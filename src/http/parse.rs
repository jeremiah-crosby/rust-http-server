use super::lex::{Lexer, Token};
use super::{HttpMethod, HttpRequest, HttpRequestBuilder};
use custom_error::custom_error;
use lazy_static::lazy_static;
use std::io::Read;
use std::str::FromStr;

custom_error! {#[derive(PartialEq)] pub ParseError
    Unexpected{msg: String} = "Unexpected token error: {msg}",
    EarlyEof = "Unexpected EOF",
    MaxHeaderSizeExceeded = "Max header size exceeded"
}

pub fn parse_from_reader(reader: Box<dyn Read>) -> Result<HttpRequest, ParseError> {
    let mut lexer = Lexer::new(reader);
    let mut request_builder = parse_request_line(&mut lexer)?;
    parse_header_lines(&mut lexer, &mut request_builder)?;
    parse_body(&mut lexer, &mut request_builder)?;

    println!("returning");
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
        Some(Token::MaxHeaderSizeExceeded) => Err(ParseError::MaxHeaderSizeExceeded),
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
            Some(Token::MaxHeaderSizeExceeded) => {
                return Err(ParseError::MaxHeaderSizeExceeded);
            }
            Some(Token::HeaderName(header_name)) => match token_iter.next() {
                Some(Token::HeaderValue(header_val)) => {
                    request_builder.with_header(header_name.as_str(), header_val.as_str());
                    return parse_header_lines(token_iter, request_builder);
                }
                Some(Token::MaxHeaderSizeExceeded) => {
                    return Err(ParseError::MaxHeaderSizeExceeded);
                }
                _ => {
                    return Err(ParseError::Unexpected {
                        msg: "Expected header value".to_string(),
                    });
                }
            },
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
        Some(other) => Err(ParseError::Unexpected {
            msg: format!("Expected body, got {:?}", other),
        }),
        None => Ok(()),
    }
}

fn parse_protocol<I>(token_iter: &mut I) -> Result<(), ParseError>
where
    I: Iterator<Item = Token>,
{
    match token_iter.next() {
        Some(Token::Protocol) => Ok(()),
        Some(Token::MaxHeaderSizeExceeded) => Err(ParseError::MaxHeaderSizeExceeded),
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
        Some(Token::MaxHeaderSizeExceeded) => Err(ParseError::MaxHeaderSizeExceeded),
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

        let request = parse_from_reader(Box::new(input.as_bytes())).unwrap();

        assert_eq!(HttpMethod::from_str("GET").unwrap(), request.method);
        assert_eq!("/", request.path);

        assert_eq!(Some(&"value1".to_string()), request.header("Header-1"));
        assert_eq!(Some(&"value2".to_string()), request.header("Header-2"));
        assert_eq!(Some(&"value3".to_string()), request.header("Header-3"));
    }

    #[test]
    fn parses_simple_valid_post_request_with_body() {
        let input = "POST / HTTP/1.1\r\n\
        Header-1: value1\r\n\
        Header-2: value2\r\n\
        Header-3: value3\r\n\
        \r\nThis is the body";

        let request = parse_from_reader(Box::new(input.as_bytes())).unwrap();

        assert_eq!(HttpMethod::from_str("POST").unwrap(), request.method);
        assert_eq!("/", request.path);

        assert_eq!(Some(&"value1".to_string()), request.header("Header-1"));
        assert_eq!(Some(&"value2".to_string()), request.header("Header-2"));
        assert_eq!(Some(&"value3".to_string()), request.header("Header-3"));

        assert_eq!("This is the body", request.body_as_string());
    }

    #[test]
    fn only_reads_content_length_bytes_of_body_if_content_length_header_used() {
        let input = "POST / HTTP/1.1\r\n\
        Content-Length: 4\r\n\
        \r\nThis is the body";

        let request = parse_from_reader(Box::new(input.as_bytes())).unwrap();

        assert_eq!(HttpMethod::from_str("POST").unwrap(), request.method);
        assert_eq!("/", request.path);

        assert_eq!("This", request.body_as_string());
    }

    #[test]
    fn parses_request_larger_than_1024_bytes() {
        lazy_static! {
            static ref INPUT: String = {
                let mut input = String::from(
                    "POST / HTTP/1.1\r\n\
                Header-1: value1\r\n\
                Header-2: value2\r\n\
                Header-3: value3\r\n\
                Content-Length: 50000\r\n\
                \r\n",
                );
                input.push_str(&"x".repeat(50000));
                input
            };
        }

        let request = parse_from_reader(Box::new(INPUT.as_bytes())).unwrap();

        assert_eq!(HttpMethod::from_str("POST").unwrap(), request.method);
        assert_eq!("/", request.path);

        assert_eq!(Some(&"value1".to_string()), request.header("Header-1"));
        assert_eq!(Some(&"value2".to_string()), request.header("Header-2"));
        assert_eq!(Some(&"value3".to_string()), request.header("Header-3"));

        assert_eq!(50000, request.body_as_string().len());
    }

    #[test]
    fn large_header_value_returns_max_header_exceeded_error() {
        lazy_static! {
            static ref INPUT: String = {
                let mut input = String::from(
                    "POST / HTTP/1.1\r\n\
                Header-1: ",
                );
                input.push_str(&"x".repeat(50000));
                input.push_str("\r\nHeader-2: value2\r\n\r\n");
                input
            };
        }

        let request = parse_from_reader(Box::new(INPUT.as_bytes()));

        if let Err(e) = request {
            assert_eq!(ParseError::MaxHeaderSizeExceeded, e);
        } else {
            panic!("Expected error, got OK");
        }
    }
}
