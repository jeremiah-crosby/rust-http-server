use lazy_static::lazy_static;
use log::trace;
use std::io::Read;
use std::str::FromStr;

use super::HttpMethod;

type LexResult = (Token, Option<LexState>);

const TOKEN_REGEX_STR: &str = r"^[!\#\$%\&'\*+-\.\^_`\|~a-zA-Z0-9]+";
const CRLF_REGEX_STR: &str = r"^\r\n";
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
}

#[derive(Debug, Clone, Copy)]
enum LexState {
    RequestLine,
    HeaderName,
    HeaderValue,
    Body,
}

pub struct Lexer {
    buffer: String,
    state: LexState,
    pos: usize,
    stream: Box<dyn Read>,
    is_eof: bool,
}
impl Iterator for Lexer {
    type Item = Token;

    fn next(&mut self) -> Option<Token> {
        if self.is_eof && self.pos >= self.buffer.len() {
            return None;
        }

        let (token, new_state) = match self.state {
            LexState::RequestLine => {
                self.fill_buffer_until_eol();
                self.lex_request_line()
            }
            LexState::HeaderName => {
                self.fill_buffer_until_eol();
                self.lex_header_name()
            }
            LexState::HeaderValue => {
                self.fill_buffer_until_eol();
                self.lex_header_value()
            }
            LexState::Body => {
                self.fill_buffer_until_eof();
                self.lex_body()
            }
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
            state: LexState::RequestLine,
            pos: 0,
            stream: reader,
            is_eof: false,
        }
    }

    fn fill_buffer_until_eol(&mut self) {
        lazy_static! {
            static ref CONTAINS_EOL_REGEX: Regex = Regex::new(r"\r\n").unwrap();
        }
        let mut contained_eol = false;
        let mut eof = false;

        while !contained_eol && !eof {
            let mut buffer = [0; 1024];

            let bytes_read = self.stream.read(&mut buffer).unwrap();
            let buffer_str = &String::from_utf8_lossy(&buffer[..bytes_read]);
            eof = bytes_read == 0;
            contained_eol = CONTAINS_EOL_REGEX.find(buffer_str).is_some();
            self.buffer.push_str(buffer_str);
        }

        self.is_eof = eof;
    }

    fn fill_buffer_until_eof(&mut self) {
        let mut eof = false;

        while !eof {
            let mut buffer = [0; 1024];

            let bytes_read = self.stream.read(&mut buffer).unwrap();
            let buffer_str = &String::from_utf8_lossy(&buffer[..bytes_read]);
            eof = bytes_read == 0;
            self.buffer.push_str(buffer_str);
        }

        self.is_eof = true;
    }

    fn lex_body(&mut self) -> LexResult {
        trace!("Lexing body");
        let body_vec = self.buffer[self.pos..].as_bytes().to_vec();
        let body_len = body_vec.len();
        let body = (Token::Body(body_vec), None);
        self.pos += body_len;
        body
    }

    fn lex_header_name(&mut self) -> LexResult {
        trace!("Lexing header name");
        match self.buffer.chars().nth(self.pos) {
            Some(c) => {
                if c == '\r' {
                    return self.lex_end_headers();
                }

                lazy_static! {
                    static ref FIELD_NAME_RE: Regex =
                        Regex::new(format!("{}:", TOKEN_REGEX_STR).as_str()).unwrap();
                }

                if let Some(mat) = (FIELD_NAME_RE).find(&self.buffer[self.pos..]) {
                    let ret = (
                        Token::HeaderName(
                            self.buffer[self.pos + mat.start()..self.pos + mat.end() - 1]
                                .to_string(),
                        ),
                        Some(LexState::HeaderValue),
                    );
                    self.pos += mat.end();
                    return ret;
                }

                (Token::Error, None)
            }
            None => (Token::Error, None),
        }
    }

    fn lex_header_value(&mut self) -> LexResult {
        trace!("Lexing header value");
        match self.buffer.chars().nth(self.pos) {
            Some(c) => {
                if c != '\r' && c.is_whitespace() {
                    self.pos += 1;
                    return self.lex_header_value();
                }

                lazy_static! {
                    static ref FIELD_VALUE_RE: Regex = Regex::new(r"^[^\r]+\r\n").unwrap();
                }

                if let Some(mat) = (FIELD_VALUE_RE).find(&self.buffer[self.pos..]) {
                    let ret = (
                        Token::HeaderValue(
                            self.buffer[self.pos + mat.start()..self.pos + mat.end() - 2]
                                .to_string(),
                        ),
                        Some(LexState::HeaderName),
                    );
                    self.pos += mat.end();
                    return ret;
                }
                (Token::Error, None)
            }
            None => (Token::Error, None),
        }
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
