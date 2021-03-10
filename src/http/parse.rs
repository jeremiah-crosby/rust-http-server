use super::{HttpMethod, HttpRequest};
use custom_error::custom_error;
use lazy_static::lazy_static;
use regex::Regex;
use std::io::Read;
use std::slice::Iter;

#[derive(Debug, PartialEq)]
enum Token {
    OneSpace,
    LinearSpaces,
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
    Headers,
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
            LexState::Headers => self.lex_headers(),
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
        reader.read_to_string(&mut buffer);
        Lexer {
            buffer,
            state: LexState::RequestLine,
            pos: 0,
        }
    }

    fn lex_headers(&mut self) -> (Token, Option<LexState>) {
        match self.buffer.chars().nth(self.pos) {
            Some(c) => {
                if c == '\r' {
                    return self.lex_end_headers();
                }

                (Token::Error, None)
            }
            None => (Token::Error, None),
        }
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
            static ref CRLF_RE: Regex = Regex::new(r"^\r\n").unwrap();
        }
        if let Some(mat) = (CRLF_RE).find(&self.buffer[self.pos..]) {
            self.pos += mat.end();
            return (Token::Crlf, Some(LexState::Headers));
        }

        (Token::Error, None)
    }

    fn lex_end_headers(&mut self) -> (Token, Option<LexState>) {
        lazy_static! {
            static ref CRLF_RE: Regex = Regex::new(r"^\r\n").unwrap();
        }
        if let Some(mat) = (CRLF_RE).find(&self.buffer[self.pos..]) {
            self.pos += mat.end();
            return (Token::Crlf, Some(LexState::Body));
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
    let request = parse_request_line(&mut lexer)?;
    parse_header_lines(&mut lexer);

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

                return Ok(HttpRequest {
                    method: method.to_string(),
                    path: path.to_string(),
                });
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

fn parse_header_lines<I>(token_iter: &mut I) -> Result<(), ParseError>
where
    I: Iterator<Item = Token>,
{
    match token_iter.next() {
        Some(Token::Crlf) => Ok(()),
        _ => parse_header_lines(token_iter),
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
        let input = "GET / HTTP/1.1\r\n\
        \r\n";
        let mut lexer = Lexer::new(&mut input.as_bytes());

        assert_eq!(Some(Token::Method("GET".to_string())), lexer.next());
        assert_eq!(Some(Token::Path("/".to_string())), lexer.next());
        assert_eq!(Some(Token::Protocol), lexer.next());
        assert_eq!(Some(Token::Crlf), lexer.next());
        assert_eq!(Some(Token::Crlf), lexer.next());
        assert_eq!(None, lexer.next());
    }

    #[test]
    fn parses_simple_valid_request() {
        let mut input = "GET / HTTP/1.1\r\n\
        \r\n";

        let request = parse_from_reader(&mut input.as_bytes()).unwrap();

        assert_eq!("GET", request.method);
        assert_eq!("/", request.path);
    }
}
