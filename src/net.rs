use custom_error::custom_error;
use std::io::Read;
use std::net::TcpListener;

custom_error! {pub NetError
    NotOpened = "TCP stream used before opened",
    IoError{source: std::io::Error} = "I/O Error: {}"
}

pub trait RequestListener {
    fn open(&mut self) -> Result<(), NetError>;
    fn accept_request(&self) -> Result<Box<dyn Read>, NetError>;
}

pub struct TcpRequestListener {
    address: String,
    port: u32,
    listener: Option<TcpListener>,
}

impl TcpRequestListener {
    pub fn new(address: &str, port: u32) -> Self {
        TcpRequestListener {
            address: address.to_owned(),
            port,
            listener: None,
        }
    }
}

impl RequestListener for TcpRequestListener {
    fn open(&mut self) -> Result<(), NetError> {
        match TcpListener::bind(format!("{}:{}", self.address, self.port)) {
            Ok(opened) => {
                self.listener = Some(opened);
                return Ok(());
            }
            Err(e) => Err(NetError::IoError { source: e }),
        }
    }

    fn accept_request(&self) -> Result<Box<dyn Read>, NetError> {
        if let Some(ref listener) = self.listener {
            match listener.accept() {
                Ok((stream, _)) => Ok(Box::new(stream)),
                Err(e) => Err(NetError::IoError { source: e }),
            }
        } else {
            Err(NetError::NotOpened)
        }
    }
}
