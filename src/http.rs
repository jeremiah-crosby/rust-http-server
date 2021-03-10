mod parse;

use std::io::Read;

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
    method: String,
    path: String,
}
