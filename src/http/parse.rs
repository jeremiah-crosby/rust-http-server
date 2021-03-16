use super::{HttpMethod, HttpRequest};
use custom_error::custom_error;
use lazy_static::lazy_static;
use regex::Regex;
use std::io::Read;
use std::slice::Iter;

const TOKEN_REGEX_STR: &str = r"^[!\#\$%\&'\*+-\.\^_`\|~a-zA-Z0-9]+";
const CRLF_REGEX_STR: &str = r"^\r\n";

#[derive(Debug, PartialEq)]
enum Token {
    Method(String),
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

struct Lexer {
    buffer: String,
    state: LexState,
    pos: usize,
}
impl Iterator for Lexer {
    type Item = Token;

    fn next(&mut self) -> Option<Token> {
        if self.pos >= self.buffer.len() {
            return None;
        }

        let (token, new_state) = match self.state {
            LexState::RequestLine => self.lex_request_line(),
            LexState::HeaderName => self.lex_header_name(),
            LexState::HeaderValue => self.lex_header_value(),
            LexState::Body => (Token::Error, None),
        };

        if let Some(state) = new_state {
            self.state = state;
        }

        Some(token)
    }
}
impl Lexer {
    pub fn new(reader: &mut Read) -> Self {
        let mut buffer = String::new();
        reader
            .read_to_string(&mut buffer)
            .expect("Did not receive UTF data for lexing");
        Lexer {
            buffer,
            state: LexState::RequestLine,
            pos: 0,
        }
    }

    fn lex_header_name(&mut self) -> (Token, Option<LexState>) {
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

    fn lex_header_value(&mut self) -> (Token, Option<LexState>) {
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

    fn lex_end_headers(&mut self) -> (Token, Option<LexState>) {
        lazy_static! {
            static ref CRLF_RE: Regex = Regex::new(CRLF_REGEX_STR).unwrap();
        }
        if let Some(mat) = (CRLF_RE).find(&self.buffer[self.pos..]) {
            self.pos += mat.end();
            return (Token::Crlf, Some(LexState::Body));
        }

        (Token::Error, None)
    }

    fn lex_request_line(&mut self) -> (Token, Option<LexState>) {
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

    fn lex_end_request_line(&mut self) -> (Token, Option<LexState>) {
        lazy_static! {
            static ref CRLF_RE: Regex = Regex::new(CRLF_REGEX_STR).unwrap();
        }
        if let Some(mat) = (CRLF_RE).find(&self.buffer[self.pos..]) {
            self.pos += mat.end();
            return (Token::Crlf, Some(LexState::HeaderName));
        }

        (Token::Error, None)
    }

    fn lex_path(&mut self) -> (Token, Option<LexState>) {
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

    fn lex_method_or_protocol(&mut self) -> (Token, Option<LexState>) {
        lazy_static! {
            static ref METHOD_RE: Regex =
                Regex::new(r"^GET|POST|PUT|PATCH|HEAD|OPTIONS|TRACE").unwrap();
            static ref PROTOCOL_RE: Regex = Regex::new(r"^HTTP/1\.1").unwrap();
        }
        if let Some(mat) = (METHOD_RE).find(&self.buffer[self.pos..]) {
            let ret = (
                Token::Method(
                    self.buffer[self.pos + mat.start()..self.pos + mat.end()].to_string(),
                ),
                None,
            );
            self.pos += mat.end();
            return ret;
        }

        if let Some(mat) = (PROTOCOL_RE).find(&self.buffer[self.pos..]) {
            let ret = (Token::Protocol, None);
            self.pos += mat.end();
            return ret;
        }

        (Token::Error, None)
    }
}

custom_error! {pub ParseError
    Unexpected{msg: String} = "Unexpected token error: {msg}",
    EarlyEof = "Unexpected EOF"
}

pub fn parse_from_reader(reader: &mut Read) -> Result<HttpRequest, ParseError> {
    let mut lexer = Lexer::new(reader);
    let mut request = parse_request_line(&mut lexer)?;
    parse_header_lines(&mut lexer, &mut request);

    Ok(request)
}

fn parse_request_line<I>(token_iter: &mut I) -> Result<HttpRequest, ParseError>
where
    I: Iterator<Item = Token>,
{
    match token_iter.next() {
        Some(Token::Method(method)) => {
            if let Some(Token::Path(path)) = token_iter.next() {
                parse_protocol(token_iter)?;
                parse_crlf(token_iter)?;

                return Ok(HttpRequest::new(method.as_str(), path.as_str()));
            }

            Err(ParseError::Unexpected {
                msg: "Expected path".to_string(),
            })
        }
        Some(_) => Err(ParseError::Unexpected {
            msg: "Expected HTTP Method".to_string(),
        }),
        _ => Err(ParseError::EarlyEof),
    }
}

fn parse_header_lines<I>(token_iter: &mut I, request: &mut HttpRequest) -> Result<(), ParseError>
where
    I: Iterator<Item = Token>,
{
    loop {
        match token_iter.next() {
            Some(Token::Crlf) => {
                return Ok(());
            }
            Some(Token::HeaderName(header_name)) => {
                if let Some(Token::HeaderValue(header_val)) = token_iter.next() {
                    request.set_header(header_name.as_str(), header_val.as_str());
                    return parse_header_lines(token_iter, request);
                }
                return Err(ParseError::Unexpected {
                    msg: "Expected header value".to_string(),
                });
            }
            _ => {
                return Err(ParseError::Unexpected {
                    msg: "Expected header".to_string(),
                });
            }
        }
    }
}

fn parse_protocol<I>(token_iter: &mut I) -> Result<(), ParseError>
where
    I: Iterator<Item = Token>,
{
    match token_iter.next() {
        Some(Token::Protocol) => Ok(()),
        Some(_) => Err(ParseError::Unexpected {
            msg: "Expected protocol version".to_string(),
        }),
        _ => Err(ParseError::EarlyEof),
    }
}

fn parse_crlf<I>(token_iter: &mut I) -> Result<(), ParseError>
where
    I: Iterator<Item = Token>,
{
    match token_iter.next() {
        Some(Token::Crlf) => Ok(()),
        Some(_) => Err(ParseError::Unexpected {
            msg: "Expected CRLF".to_string(),
        }),
        _ => Err(ParseError::EarlyEof),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lexes_valid_GET_request_line() {
        let input = "GET / HTTP/1.1\r\nHeader-1: value\r\nAnother-Header: different value\r\n\r\n";
        let mut lexer = Lexer::new(&mut input.as_bytes());

        assert_eq!(Some(Token::Method("GET".to_string())), lexer.next());
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

        assert_eq!(None, lexer.next());
    }

    #[test]
    fn parses_simple_valid_GET_request() {
        let mut input = "GET / HTTP/1.1\r\n\
        Header-1: value1\r\n\
        Header-2: value2\r\n\
        Header-3: value3\r\n\
        \r\n";

        let request = parse_from_reader(&mut input.as_bytes()).unwrap();

        assert_eq!("GET", request.method);
        assert_eq!("/", request.path);

        assert_eq!(Some(&"value1".to_string()), request.header("Header-1"));
        assert_eq!(Some(&"value2".to_string()), request.header("Header-2"));
        assert_eq!(Some(&"value3".to_string()), request.header("Header-3"));
    }
}
