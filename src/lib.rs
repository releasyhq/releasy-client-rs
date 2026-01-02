#![doc = include_str!("../README.md")]

mod client;
mod error;
mod models;

pub use crate::client::{Auth, Client, ClientBuilder, DownloadResolution};
pub use crate::error::{Error, Result};
pub use crate::models::*;
