use data::Moc3;
use puppet::{Puppet, puppet_from_moc3};
use thiserror::Error;

pub mod data;

mod deformer;
mod math;
pub mod owned;
pub mod puppet;

#[derive(Error, Debug)]
#[error("could not parse moc3")]
pub struct ParseError;

pub fn parse_puppet(bytes: &[u8]) -> Result<Puppet, ParseError> {
    let read = Moc3::new(bytes).map_err(|_| ParseError)?;
    Ok(puppet_from_moc3(read))
}
