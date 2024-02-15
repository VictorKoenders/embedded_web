use crate::{Error, Result};
use core::{ascii::Char as AsciiChar, str::FromStr};

pub fn read_http_request_line(data: &[u8]) -> Result<(HttpMethod, &str, &[u8])> {
    let (line, remaining) =
        read_ascii_until(data, AsciiChar::LineFeed).ok_or(Error::InvalidRequestLine)?;
    let (method, line) = line.split_once(' ').ok_or(Error::InvalidRequestLine)?;
    let method = HttpMethod::from_str(method)?;

    let (url, _http_version) = line.rsplit_once(' ').ok_or(Error::InvalidRequestLine)?;
    // TODO: validate http version?
    Ok((method, url, remaining))
}

#[test]
fn test_read_http_request_line() {
    assert_eq!(read_http_request_line(&[]), Err(Error::InvalidRequestLine));
    assert_eq!(
        read_http_request_line(&b"\xe5\n"[..]),
        Err(Error::InvalidRequestLine)
    );
    assert_eq!(
        read_http_request_line(&b"GET /foo/bar HTTP/1.1\n"[..]).unwrap(),
        (HttpMethod::Get, "/foo/bar", &[][..])
    );
    assert_eq!(
        read_http_request_line(&b"GET this is a weird url HTTP/1.1\n"[..]).unwrap(),
        (HttpMethod::Get, "this is a weird url", &[][..])
    );
}

pub fn read_header_line(data: &[u8]) -> HeaderLine {
    if let Some(remaining) = data.strip_prefix(b"\r\n") {
        return HeaderLine::Whiteline { remaining };
    }

    let Some((key, remaining)) = read_ascii_until(data, AsciiChar::Colon) else {
        return HeaderLine::Error(Error::InvalidHttpHeader);
    };
    let Some((value, remaining)) = read_ascii_until(remaining, AsciiChar::LineFeed) else {
        return HeaderLine::Error(Error::InvalidHttpHeader);
    };
    HeaderLine::Header {
        key: key.trim(),
        value: value.trim(),
        remaining,
    }
}

pub enum HeaderLine<'a> {
    Error(super::Error),
    Whiteline {
        remaining: &'a [u8],
    },
    Header {
        key: &'a str,
        value: &'a str,
        remaining: &'a [u8],
    },
}

fn read_ascii_until(data: &[u8], char: AsciiChar) -> Option<(&str, &[u8])> {
    for (i, b) in data.iter().enumerate() {
        let c = AsciiChar::from_u8(*b)?;
        if c == char {
            return Some((
                unsafe { core::str::from_utf8_unchecked(&data[..i]) }, // safe because it's all ascii characters
                &data[i + 1..],
            ));
        }
    }
    None
}

#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Debug, Hash)]
pub enum HttpMethod {
    Get,
    Head,
    Post,
    Put,
    Delete,
    Connect,
    Options,
    Trace,
    Patch,
}

impl FromStr for HttpMethod {
    type Err = Error;

    fn from_str(s: &str) -> Result<Self> {
        match s {
            "get" | "Get" | "GET" => Ok(Self::Get),
            "head" | "Head" | "HEAD" => Ok(Self::Head),
            "post" | "Post" | "POST" => Ok(Self::Post),
            "put" | "Put" | "PUT" => Ok(Self::Put),
            "delete" | "Delete" | "DELETE" => Ok(Self::Delete),
            "connect" | "Connect" | "CONNECT" => Ok(Self::Connect),
            "options" | "Options" | "OPTIONS" => Ok(Self::Options),
            "trace" | "Trace" | "TRACE" => Ok(Self::Trace),
            "patch" | "Patch" | "PATCH" => Ok(Self::Patch),
            _ => Err(Error::InvalidHttpMethod),
        }
    }
}
