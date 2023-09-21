use std::io::Cursor;

use binrw::BinReaderExt;
use data::Moc3Data;
use puppet::{puppet_from_moc3, Puppet};
use thiserror::Error;

pub mod data;
pub mod deformer;
pub mod interpolate;
mod math;
pub mod puppet;

#[derive(Error, Debug)]
#[error("could not parse moc3")]
pub struct ParseError;

pub fn parse_puppet(bytes: &[u8]) -> Result<Puppet, ParseError> {
    let mut cursor = Cursor::new(bytes);
    let read: Moc3Data = cursor.read_le().map_err(|_| ParseError)?;
    Ok(puppet_from_moc3(&read))
}
