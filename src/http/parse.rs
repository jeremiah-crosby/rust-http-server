use super::lex::{Lexer, Token};
use super::{HttpMethod, HttpRequest, HttpRequestBuilder};
use custom_error::custom_error;
use std::io::Read;
use std::str::FromStr;

custom_error! {pub ParseError
    Unexpected{msg: String} = "Unexpected token error: {msg}",
    EarlyEof = "Unexpected EOF"
}

pub fn parse_from_reader(reader: &mut dyn Read) -> Result<HttpRequest, ParseError> {
    let mut lexer = Lexer::new(reader);
    let mut request_builder = parse_request_line(&mut lexer)?;
    let headers = parse_header_lines(&mut lexer, &mut request_builder)?;
    parse_body(&mut lexer, &mut request_builder);

    Ok(request_builder.build())
}

fn parse_request_line<I>(token_iter: &mut I) -> Result<HttpRequestBuilder, ParseError>
where
    I: Iterator<Item = Token>,
{
    match token_iter.next() {
        Some(Token::Method(method)) => {
            if let Some(Token::Path(path)) = token_iter.next() {
                parse_protocol(token_iter)?;
                parse_crlf(token_iter)?;

                let mut builder = HttpRequestBuilder::new();
                builder.with_method(method);
                builder.with_path(&path);

                return Ok(builder);
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

fn parse_header_lines<I>(
    token_iter: &mut I,
    request_builder: &mut HttpRequestBuilder,
) -> Result<(), ParseError>
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
                    request_builder.with_header(header_name.as_str(), header_val.as_str());
                    return parse_header_lines(token_iter, request_builder);
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

fn parse_body<I>(
    token_iter: &mut I,
    request_builder: &mut HttpRequestBuilder,
) -> Result<(), ParseError>
where
    I: Iterator<Item = Token>,
{
    match token_iter.next() {
        Some(Token::Body(ref content)) => {
            request_builder.with_body(content);
            Ok(())
        }
        _ => Err(ParseError::Unexpected {
            msg: "Expected body".to_string(),
        }),
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
    fn parses_simple_valid_get_request() {
        let input = "GET / HTTP/1.1\r\n\
        Header-1: value1\r\n\
        Header-2: value2\r\n\
        Header-3: value3\r\n\
        \r\n";

        let request = parse_from_reader(&mut input.as_bytes()).unwrap();

        assert_eq!(HttpMethod::from_str("GET").unwrap(), request.method);
        assert_eq!("/", request.path);

        assert_eq!(Some(&"value1".to_string()), request.header("Header-1"));
        assert_eq!(Some(&"value2".to_string()), request.header("Header-2"));
        assert_eq!(Some(&"value3".to_string()), request.header("Header-3"));
    }

    #[test]
    fn parses_simple_valid_post_request_with_body() {
        let mut input = "POST / HTTP/1.1\r\n\
        Header-1: value1\r\n\
        Header-2: value2\r\n\
        Header-3: value3\r\n\
        \r\nThis is the body";

        let request = parse_from_reader(&mut input.as_bytes()).unwrap();

        assert_eq!(HttpMethod::from_str("POST").unwrap(), request.method);
        assert_eq!("/", request.path);

        assert_eq!(Some(&"value1".to_string()), request.header("Header-1"));
        assert_eq!(Some(&"value2".to_string()), request.header("Header-2"));
        assert_eq!(Some(&"value3".to_string()), request.header("Header-3"));

        assert_eq!("This is the body", request.body_as_string());
    }
}
