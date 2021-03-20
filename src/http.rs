mod lex;
mod parse;

use std::collections::HashMap;

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

pub struct HttpRequest {
    pub method: String,
    pub path: String,
    headers: HashMap<String, String>,
}
impl HttpRequest {
    pub fn new(method: &str, path: &str) -> Self {
        HttpRequest {
            method: method.to_owned(),
            path: path.to_owned(),
            headers: HashMap::new(),
        }
    }

    pub fn set_header(&mut self, name: &str, value: &str) {
        self.headers.insert(name.to_owned(), value.to_owned());
    }

    pub fn header(&self, name: &str) -> Option<&String> {
        self.headers.get(name)
    }
}
