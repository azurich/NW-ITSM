//! Address of the client, for the login limits and the event log.

use std::{
    convert::Infallible,
    net::{IpAddr, SocketAddr},
};

use axum::{
    extract::{ConnectInfo, FromRequestParts},
    http::request::Parts,
};

use crate::state::AppState;

/// Client IP address, or `unknown`.
#[derive(Debug, Clone)]
pub struct ClientAddress(pub String);

impl FromRequestParts<AppState> for ClientAddress {
    type Rejection = Infallible;

    async fn from_request_parts(parts: &mut Parts, state: &AppState) -> Result<Self, Infallible> {
        if state.trust_proxy {
            // The trusted proxy appends the address it received the request from.
            let forwarded = parts
                .headers
                .get("x-forwarded-for")
                .and_then(|value| value.to_str().ok())
                .and_then(|value| value.rsplit(',').next())
                .and_then(|last| last.trim().parse::<IpAddr>().ok());
            if let Some(address) = forwarded {
                return Ok(Self(address.to_string()));
            }
        }
        let address = parts
            .extensions
            .get::<ConnectInfo<SocketAddr>>()
            .map(|ConnectInfo(address)| address.ip().to_string())
            .unwrap_or_else(|| "unknown".to_owned());
        Ok(Self(address))
    }
}
