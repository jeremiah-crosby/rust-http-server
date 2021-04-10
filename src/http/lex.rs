use lazy_static::lazy_static;
use log::trace;
use std::io::Read;
use std::str::FromStr;

use super::HttpMethod;

type LexResult = (Token, Option<LexState>);

const TOKEN_REGEX_STR: &str = r"^[!\#\$%\&'\*+-\.\^_`\|~a-zA-Z0-9]+";
const CRLF_REGEX_STR: &str = r"^\r\n";
const MAX_HEADER_SIZE: usize = 1024 * 8;
use regex::Regex;

#[derive(Debug, PartialEq)]
pub enum Token {
    Method(HttpMethod),
    Path(String),
    Protocol,
    HeaderName(String),
    HeaderValue(String),
    Body(Vec<u8>),
    Crlf,
    Error,
    MaxHeaderSizeExceeded,
}

#[derive(Debug, Clone, Copy)]
enum LexState {
    Initial,
    RequestLine,
    HeaderName,
    HeaderValue,
    Body,
    End,
}

pub struct Lexer {
    buffer: String,
    state: LexState,
    pos: usize,
    stream: Box<dyn Read>,
    is_eof: bool,
    expecting_content_length: bool,
    content_length: Option<usize>,
}
impl Iterator for Lexer {
    type Item = Token;

    fn next(&mut self) -> Option<Token> {
        if self.is_eof && self.pos >= self.buffer.len() {
            return None;
        }

        let (token, new_state) = match self.state {
            LexState::Initial => {
                self.refill_buffer();
                self.state = LexState::RequestLine;
                self.lex_request_line()
            }
            LexState::RequestLine => {
                if self.header_size_exceeded() {
                    return Some(Token::MaxHeaderSizeExceeded);
                }
                self.lex_request_line()
            }
            LexState::HeaderName => {
                if self.header_size_exceeded() {
                    return Some(Token::MaxHeaderSizeExceeded);
                }
                self.lex_header_name()
            }
            LexState::HeaderValue => {
                if self.header_size_exceeded() {
                    return Some(Token::MaxHeaderSizeExceeded);
                }
                self.lex_header_value()
            }
            LexState::Body => {
                self.fill_buffer_until_content_length_or_eof();
                self.lex_body()
            }
            LexState::End => return None,
        };

        if let Some(state) = new_state {
            self.state = state;
        }

        Some(token)
    }
}
impl Lexer {
    pub fn new(reader: Box<dyn Read>) -> Self {
        Lexer {
            buffer: String::new(),
            state: LexState::Initial,
            pos: 0,
            stream: reader,
            is_eof: false,
            expecting_content_length: false,
            content_length: None,
        }
    }

    fn header_size_exceeded(&self) -> bool {
        self.pos > MAX_HEADER_SIZE
    }

    fn refill_buffer(&mut self) {
        let mut buffer = [0; 1024];
        let bytes_read = self.stream.read(&mut buffer).unwrap();
        let buffer_str = &String::from_utf8_lossy(&buffer[..bytes_read]);
        self.is_eof = bytes_read == 0;
        self.buffer.push_str(buffer_str);
    }

    fn fill_buffer_until_max_header_size(&mut self) {
        let mut buf = String::new();

        self.stream
            .by_ref()
            .take(MAX_HEADER_SIZE as u64)
            .read_to_string(&mut buf)
            .unwrap();
        self.buffer.push_str(&buf);
    }

    fn fill_buffer_until_content_length_or_eof(&mut self) {
        if self.is_eof || self.content_length.is_none() {
            return;
        }

        let mut eof = false;

        if let Some(content_length) = self.content_length {
            let mut buf = String::new();

            self.stream
                .by_ref()
                .take(content_length as u64)
                .read_to_string(&mut buf)
                .unwrap();
            self.buffer.push_str(&buf);
        } else {
            while !eof {
                let mut buffer = [0; 1024];
                let bytes_read = self.stream.read(&mut buffer).unwrap();
                let buffer_str = &String::from_utf8_lossy(&buffer[..bytes_read]);
                eof = bytes_read == 0;
                self.buffer.push_str(buffer_str);
            }
        }

        self.is_eof = true;
    }

    fn lex_body(&mut self) -> LexResult {
        trace!("Lexing body");
        let body_vec = match self.content_length {
            Some(content_length) => self.buffer[self.pos..self.pos + content_length]
                .as_bytes()
                .to_vec(),
            _ => self.buffer[self.pos..].as_bytes().to_vec(),
        };
        let body_len = body_vec.len();
        let body = (Token::Body(body_vec), Some(LexState::End));
        self.pos += body_len;
        body
    }

    fn lex_header_name(&mut self) -> LexResult {
        trace!("Lexing header name");
        if self.buffer.chars().nth(self.pos) == Some('\r') {
            return self.lex_end_headers();
        }
        let start_pos = self.pos;
        loop {
            match self.buffer.chars().nth(self.pos) {
                Some(c) => {
                    if c == ':' {
                        let name = &self.buffer[start_pos..self.pos].to_owned();
                        self.pos += 1;
                        self.expecting_content_length = name.to_lowercase() == "content-length";
                        return (
                            Token::HeaderName(name.to_string()),
                            Some(LexState::HeaderValue),
                        );
                    }

                    if self.header_size_exceeded() {
                        return (Token::MaxHeaderSizeExceeded, None);
                    }

                    if self.is_valid_header_name_char(c) {
                        self.pos += 1;
                        continue;
                    }

                    return (Token::Error, None);
                }
                None => {
                    self.refill_buffer();
                    continue;
                }
            }
        }
    }

    fn is_valid_header_name_char(&self, c: char) -> bool {
        c.is_alphanumeric() || c == '-'
    }

    fn lex_header_value(&mut self) -> LexResult {
        trace!("Lexing header value");
        let start_pos = self.pos;
        loop {
            match self.buffer.chars().nth(self.pos) {
                Some(c) => {
                    if c == '\r' {
                        let value = &self.buffer[start_pos..self.pos].to_owned();
                        return self.lex_end_header_value(value);
                    }

                    if self.header_size_exceeded() {
                        return (Token::MaxHeaderSizeExceeded, None);
                    }

                    if self.is_valid_header_value_char(c) {
                        self.pos += 1;
                        continue;
                    }

                    return (Token::Error, None);
                }
                None => {
                    self.refill_buffer();
                    continue;
                }
            }
        }
    }

    fn lex_end_header_value(&mut self, value: &str) -> LexResult {
        lazy_static! {
            static ref CRLF_RE: Regex = Regex::new(CRLF_REGEX_STR).unwrap();
        }
        if let Some(mat) = (CRLF_RE).find(&self.buffer[self.pos..]) {
            self.pos += mat.end();
            if self.expecting_content_length {
                self.expecting_content_length = false;
                if let Ok(content_length) = value.trim_start().parse::<usize>() {
                    self.content_length = Some(content_length);
                }
            }
            return (
                Token::HeaderValue(value.trim_start().to_owned()),
                Some(LexState::HeaderName),
            );
        }

        (Token::Error, None)
    }

    fn is_valid_header_value_char(&self, c: char) -> bool {
        c != '\r' && c != '\n'
    }

    fn lex_end_headers(&mut self) -> LexResult {
        trace!("Lexing end of headers");
        lazy_static! {
            static ref CRLF_RE: Regex = Regex::new(CRLF_REGEX_STR).unwrap();
        }
        if let Some(mat) = (CRLF_RE).find(&self.buffer[self.pos..]) {
            self.pos += mat.end();
            return (Token::Crlf, Some(LexState::Body));
        }

        (Token::Error, None)
    }

    fn lex_request_line(&mut self) -> LexResult {
        trace!("Lexing request line");
        match self.buffer.chars().nth(self.pos) {
            Some(c) => {
                if c == '\r' {
                    return self.lex_end_request_line();
                }

                if c.is_whitespace() {
                    self.pos += 1;
                    return self.lex_request_line();
                }

                if c.is_alphabetic() {
                    return self.lex_method_or_protocol();
                }

                if c == '/' {
                    return self.lex_path();
                }

                (Token::Error, None)
            }
            None => (Token::Error, None),
        }
    }

    fn lex_end_request_line(&mut self) -> LexResult {
        trace!("Lexing end of request line");
        lazy_static! {
            static ref CRLF_RE: Regex = Regex::new(CRLF_REGEX_STR).unwrap();
        }
        if let Some(mat) = (CRLF_RE).find(&self.buffer[self.pos..]) {
            self.pos += mat.end();
            return (Token::Crlf, Some(LexState::HeaderName));
        }

        (Token::Error, None)
    }

    fn lex_path(&mut self) -> LexResult {
        trace!("Lexing request path");
        lazy_static! {
            static ref PATH_RE: Regex = Regex::new(r"^[a-z0-9\-._~%!$&'()*+,;=:@/]+").unwrap();
        }
        if let Some(mat) = (PATH_RE).find(&self.buffer[self.pos..]) {
            let ret = (
                Token::Path(self.buffer[self.pos + mat.start()..self.pos + mat.end()].to_string()),
                None,
            );
            self.pos += mat.end();
            return ret;
        }

        (Token::Error, None)
    }

    fn lex_method_or_protocol(&mut self) -> LexResult {
        lazy_static! {
            static ref METHOD_RE: Regex =
                Regex::new(r"^GET|POST|PUT|PATCH|HEAD|OPTIONS|TRACE").unwrap();
            static ref PROTOCOL_RE: Regex = Regex::new(r"^HTTP/1\.1").unwrap();
        }
        if let Some(mat) = (METHOD_RE).find(&self.buffer[self.pos..]) {
            trace!("Lexing request method");

            let ret = (
                Token::Method(
                    HttpMethod::from_str(
                        &self.buffer[self.pos + mat.start()..self.pos + mat.end()],
                    )
                    .unwrap(),
                ),
                None,
            );
            self.pos += mat.end();
            return ret;
        }

        if let Some(mat) = (PROTOCOL_RE).find(&self.buffer[self.pos..]) {
            trace!("Lexing request protocol and version");

            let ret = (Token::Protocol, None);
            self.pos += mat.end();
            return ret;
        }

        (Token::Error, None)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lexes_valid_get_request_line() {
        let input = "GET / HTTP/1.1\r\nHeader-1: value\r\nAnother-Header: different value\r\n\r\n";
        let mut lexer = Lexer::new(Box::new(input.as_bytes()));

        assert_eq!(
            Some(Token::Method(HttpMethod::from_str("GET").unwrap())),
            lexer.next()
        );
        assert_eq!(Some(Token::Path("/".to_string())), lexer.next());
        assert_eq!(Some(Token::Protocol), lexer.next());
        assert_eq!(Some(Token::Crlf), lexer.next());

        assert_eq!(
            Some(Token::HeaderName("Header-1".to_string())),
            lexer.next()
        );
        assert_eq!(Some(Token::HeaderValue("value".to_string())), lexer.next());

        assert_eq!(
            Some(Token::HeaderName("Another-Header".to_string())),
            lexer.next()
        );
        assert_eq!(
            Some(Token::HeaderValue("different value".to_string())),
            lexer.next()
        );
        assert_eq!(Some(Token::Crlf), lexer.next());

        lexer.next();
        assert_eq!(None, lexer.next());
    }

    #[test]
    fn lexes_path_with_period() {
        let input = "GET /static/test.txt HTTP/1.1\r\nHeader-1: value\r\nAnother-Header: different value\r\n\r\n";
        let mut lexer = Lexer::new(Box::new(input.as_bytes()));

        assert_eq!(
            Some(Token::Method(HttpMethod::from_str("GET").unwrap())),
            lexer.next()
        );
        assert_eq!(
            Some(Token::Path("/static/test.txt".to_string())),
            lexer.next()
        );
        assert_eq!(Some(Token::Protocol), lexer.next());
        assert_eq!(Some(Token::Crlf), lexer.next());

        assert_eq!(
            Some(Token::HeaderName("Header-1".to_string())),
            lexer.next()
        );
        assert_eq!(Some(Token::HeaderValue("value".to_string())), lexer.next());

        assert_eq!(
            Some(Token::HeaderName("Another-Header".to_string())),
            lexer.next()
        );
        assert_eq!(
            Some(Token::HeaderValue("different value".to_string())),
            lexer.next()
        );
        assert_eq!(Some(Token::Crlf), lexer.next());

        lexer.next();

        assert_eq!(None, lexer.next());
    }
}
