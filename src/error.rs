// Copyright 2020 Benjamin Scherer
// Licensed under the Open Software License version 3.0

use reqwest::Error as ReqwestError;
use rusqlite::Error as RusqliteError;
use serenity::Error as SerenityError;

use std::{
	error::Error as StdError,
	fmt::{self, Display},
};

#[derive(Debug)]
struct SimpleError(String);

impl Display for SimpleError {
	fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
		Display::fmt(&self.0, f)
	}
}

impl StdError for SimpleError {}

#[derive(Debug)]
pub struct Error(Box<dyn StdError + Send + Sync + 'static>);

impl From<SerenityError> for Error {
	fn from(e: SerenityError) -> Self {
		Self(Box::new(e))
	}
}

impl From<RusqliteError> for Error {
	fn from(e: RusqliteError) -> Self {
		Self(Box::new(e))
	}
}

impl From<ReqwestError> for Error {
	fn from(e: ReqwestError) -> Self {
		Self(Box::new(e))
	}
}

impl From<String> for Error {
	fn from(e: String) -> Self {
		Self(Box::new(SimpleError(e)))
	}
}

impl From<&'_ str> for Error {
	fn from(e: &str) -> Self {
		Self(Box::new(SimpleError(e.to_owned())))
	}
}

impl Display for Error {
	fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
		Display::fmt(&self.0, f)
	}
}

impl StdError for Error {}
impl StdError for &'_ Error {}
