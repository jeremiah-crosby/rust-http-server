mod lex;
mod parse;

pub use self::parse::{parse_from_reader, ParseError};

use std::collections::HashMap;
use std::str::FromStr;

#[derive(Copy, Clone, Debug, PartialEq)]
pub enum HttpMethod {
    GET,
    HEAD,
    POST,
    PUT,
    PATCH,
    OPTIONS,
    TRACE,
}
impl FromStr for HttpMethod {
    type Err = ();

    fn from_str(input: &str) -> Result<HttpMethod, Self::Err> {
        match input {
            "GET" => Ok(HttpMethod::GET),
            "HEAD" => Ok(HttpMethod::HEAD),
            "POST" => Ok(HttpMethod::POST),
            "PUT" => Ok(HttpMethod::PUT),
            "PATCH" => Ok(HttpMethod::PATCH),
            "OPTIONS" => Ok(HttpMethod::OPTIONS),
            "TRACE" => Ok(HttpMethod::TRACE),
            _ => Err(()),
        }
    }
}

#[derive(Debug)]
struct HttpBody {
    content: Vec<u8>,
}
impl HttpBody {
    pub fn new() -> Self {
        HttpBody {
            content: Vec::new(),
        }
    }

    pub fn from_content(content: &Vec<u8>) -> Self {
        HttpBody {
            content: content.clone(),
        }
    }

    pub fn as_str(&self) -> &str {
        std::str::from_utf8(&self.content).unwrap()
    }
}

#[derive(Debug)]
pub struct HttpRequest {
    pub method: HttpMethod,
    pub path: String,
    headers: HashMap<String, String>,
    body: HttpBody,
}
impl HttpRequest {
    pub fn new(method: HttpMethod, path: &str) -> Self {
        HttpRequest {
            method,
            path: path.to_owned(),
            headers: HashMap::new(),
            body: HttpBody::new(),
        }
    }

    pub fn set_header(&mut self, name: &str, value: &str) {
        self.headers.insert(name.to_owned(), value.to_owned());
    }

    pub fn header(&self, name: &str) -> Option<&String> {
        self.headers.get(name)
    }

    pub fn body_as_string(&self) -> &str {
        self.body.as_str()
    }
}

pub struct HttpRequestBuilder {
    method: HttpMethod,
    path: String,
    headers: HashMap<String, String>,
    body: HttpBody,
}
impl HttpRequestBuilder {
    pub fn new() -> Self {
        HttpRequestBuilder {
            method: HttpMethod::GET,
            path: String::new(),
            headers: HashMap::new(),
            body: HttpBody::new(),
        }
    }

    pub fn with_method(&mut self, method: HttpMethod) -> &mut HttpRequestBuilder {
        self.method = method;
        self
    }

    pub fn with_path(&mut self, path: &str) -> &mut HttpRequestBuilder {
        self.path = path.to_string();
        self
    }

    pub fn with_header(&mut self, name: &str, value: &str) -> &mut HttpRequestBuilder {
        self.headers.insert(name.to_owned(), value.to_owned());
        self
    }

    pub fn with_body(&mut self, content: &Vec<u8>) -> &mut HttpRequestBuilder {
        self.body = HttpBody::from_content(content);
        self
    }

    pub fn build(self) -> HttpRequest {
        HttpRequest {
            method: self.method,
            path: self.path.to_string(),
            headers: self.headers,
            body: self.body,
        }
    }
}
