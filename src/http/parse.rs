use super::lex::{Lexer, Token};
use super::{HttpMethod, HttpRequest, HttpRequestBuilder};
use custom_error::custom_error;
use lazy_static::lazy_static;
use std::str::FromStr;
use tokio::io::AsyncReadExt;

custom_error! {#[derive(PartialEq)] pub ParseError
    Unexpected{msg: String} = "Unexpected token error: {msg}",
    EarlyEof = "Unexpected EOF",
    MaxHeaderSizeExceeded = "Max header size exceeded"
}

pub async fn parse_from_reader<T>(reader: &mut T) -> Result<HttpRequest, ParseError>
where
    T: AsyncReadExt + Unpin,
{
    let mut lexer = Lexer::new(reader);
    let mut request_builder = parse_request_line(&mut lexer).await?;
    let mut parsing_headers = true;

    while parsing_headers {
        parsing_headers = parse_header_lines(&mut lexer, &mut request_builder).await?;
    }
    parse_body(&mut lexer, &mut request_builder).await?;

    Ok(request_builder.build())
}

async fn parse_request_line<'a, T>(
    token_iter: &mut Lexer<'a, T>,
) -> Result<HttpRequestBuilder, ParseError>
where
    T: AsyncReadExt + Unpin,
{
    match token_iter.next().await {
        Some(Token::Method(method)) => {
            if let Some(Token::Path(path)) = token_iter.next().await {
                parse_protocol(token_iter).await?;
                parse_crlf(token_iter).await?;

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

async fn parse_header_lines<'a, T>(
    token_iter: &mut Lexer<'a, T>,
    request_builder: &mut HttpRequestBuilder,
) -> Result<bool, ParseError>
where
    T: AsyncReadExt + Unpin,
{
    loop {
        match token_iter.next().await {
            Some(Token::Crlf) => {
                return Ok(false);
            }
            Some(Token::MaxHeaderSizeExceeded) => {
                return Err(ParseError::MaxHeaderSizeExceeded);
            }
            Some(Token::HeaderName(header_name)) => match token_iter.next().await {
                Some(Token::HeaderValue(header_val)) => {
                    request_builder.with_header(header_name.as_str(), header_val.as_str());
                    return Ok(true);
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

async fn parse_body<'a, T>(
    token_iter: &mut Lexer<'a, T>,
    request_builder: &mut HttpRequestBuilder,
) -> Result<(), ParseError>
where
    T: AsyncReadExt + Unpin,
{
    match token_iter.next().await {
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

async fn parse_protocol<'a, T>(token_iter: &mut Lexer<'a, T>) -> Result<(), ParseError>
where
    T: AsyncReadExt + Unpin,
{
    match token_iter.next().await {
        Some(Token::Protocol) => Ok(()),
        Some(Token::MaxHeaderSizeExceeded) => Err(ParseError::MaxHeaderSizeExceeded),
        Some(_) => Err(ParseError::Unexpected {
            msg: "Expected protocol version".to_string(),
        }),
        _ => Err(ParseError::EarlyEof),
    }
}

async fn parse_crlf<'a, T>(token_iter: &mut Lexer<'a, T>) -> Result<(), ParseError>
where
    T: AsyncReadExt + Unpin,
{
    match token_iter.next().await {
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

    #[tokio::test]
    async fn parses_simple_valid_get_request() {
        let mut input = "GET / HTTP/1.1\r\n\
        Header-1: value1\r\n\
        Header-2: value2\r\n\
        Header-3: value3\r\n\
        \r\n";

        let request = (parse_from_reader(&mut input.as_bytes()).await).unwrap();

        assert_eq!(HttpMethod::from_str("GET").unwrap(), request.method);
        assert_eq!("/", request.path);

        assert_eq!(Some(&"value1".to_string()), request.header("Header-1"));
        assert_eq!(Some(&"value2".to_string()), request.header("Header-2"));
        assert_eq!(Some(&"value3".to_string()), request.header("Header-3"));
    }

    #[tokio::test]
    async fn parses_simple_valid_post_request_with_body() {
        let mut input = "POST / HTTP/1.1\r\n\
        Header-1: value1\r\n\
        Header-2: value2\r\n\
        Header-3: value3\r\n\
        \r\nThis is the body";

        let request = (parse_from_reader(&mut input.as_bytes()).await).unwrap();

        assert_eq!(HttpMethod::from_str("POST").unwrap(), request.method);
        assert_eq!("/", request.path);

        assert_eq!(Some(&"value1".to_string()), request.header("Header-1"));
        assert_eq!(Some(&"value2".to_string()), request.header("Header-2"));
        assert_eq!(Some(&"value3".to_string()), request.header("Header-3"));

        assert_eq!("This is the body", request.body_as_string());
    }

    #[tokio::test]
    async fn only_reads_content_length_bytes_of_body_if_content_length_header_used() {
        let mut input = "POST / HTTP/1.1\r\n\
        Content-Length: 4\r\n\
        \r\nThis is the body";

        let request = (parse_from_reader(&mut input.as_bytes()).await).unwrap();

        assert_eq!(HttpMethod::from_str("POST").unwrap(), request.method);
        assert_eq!("/", request.path);

        assert_eq!("This", request.body_as_string());
    }

    #[tokio::test]
    async fn parses_request_larger_than_1024_bytes() {
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

        let request = (parse_from_reader(&mut INPUT.as_bytes()).await).unwrap();

        assert_eq!(HttpMethod::from_str("POST").unwrap(), request.method);
        assert_eq!("/", request.path);

        assert_eq!(Some(&"value1".to_string()), request.header("Header-1"));
        assert_eq!(Some(&"value2".to_string()), request.header("Header-2"));
        assert_eq!(Some(&"value3".to_string()), request.header("Header-3"));

        assert_eq!(50000, request.body_as_string().len());
    }

    #[tokio::test]
    async fn large_header_value_returns_max_header_exceeded_error() {
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

        let request = parse_from_reader(&mut INPUT.as_bytes()).await;

        if let Err(e) = request {
            assert_eq!(ParseError::MaxHeaderSizeExceeded, e);
        } else {
            panic!("Expected error, got OK");
        }
    }
}
