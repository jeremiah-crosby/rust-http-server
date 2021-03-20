use super::lex::{Lexer, Token};
use super::HttpRequest;
use custom_error::custom_error;
use std::io::Read;

custom_error! {pub ParseError
    Unexpected{msg: String} = "Unexpected token error: {msg}",
    EarlyEof = "Unexpected EOF"
}

pub fn parse_from_reader(reader: &mut dyn Read) -> Result<HttpRequest, ParseError> {
    let mut lexer = Lexer::new(reader);
    let mut request = parse_request_line(&mut lexer)?;
    parse_header_lines(&mut lexer, &mut request)?;

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
