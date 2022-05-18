// Copyright 2022 ThatsNoMoon
// Licensed under the Open Software License version 3.0

//! Interface for interacting with the sqlite database of keywords and other persistent user
//! information.

#![cfg_attr(not(feature = "bot"), allow(dead_code))]

mod backup;
mod block;
mod ignore;
mod keyword;
mod mute;
mod notification;
mod opt_out;
mod user_state;

use std::{fs, io::ErrorKind};

use once_cell::sync::OnceCell;
use r2d2::{Pool, PooledConnection};
use r2d2_sqlite::SqliteConnectionManager;
use rusqlite::OpenFlags;
use serenity::model::id::{ChannelId, GuildId, MessageId, UserId};

use self::backup::start_backup_cycle;
pub(crate) use self::{
	block::Block, ignore::Ignore, keyword::Keyword, mute::Mute,
	notification::Notification, opt_out::OptOut, user_state::UserState,
};
#[cfg(feature = "bot")]
pub(crate) use self::{keyword::KeywordKind, user_state::UserStateKind};
use crate::settings::settings;

/// Global connection pool to the database.
static POOL: OnceCell<Pool<SqliteConnectionManager>> = OnceCell::new();

/// Gets a connection from the global connection pool.
#[tracing::instrument]
pub(crate) fn connection() -> PooledConnection<SqliteConnectionManager> {
	POOL.get()
		.expect("Database pool was not initialized")
		.get()
		.expect("Failed to obtain database connection")
}

/// Initializes the database.
///
/// Creates the data folder and database file if necessary, and starts backups
/// if enabled.
pub(crate) fn init() {
	let data_dir = &settings().database.path;

	if let Err(error) = fs::create_dir(data_dir) {
		if error.kind() != ErrorKind::AlreadyExists {
			Err::<(), _>(error).expect("Failed to create data directory");
		}
	}

	let manager = SqliteConnectionManager::file(data_dir.join("data.db"))
		.with_flags(
			OpenFlags::SQLITE_OPEN_CREATE | OpenFlags::SQLITE_OPEN_READ_WRITE,
		);

	let pool = Pool::new(manager).expect("Failed to open database pool");

	POOL.set(pool).unwrap();

	Mute::create_table();
	Block::create_table();
	Ignore::create_table();
	OptOut::create_table();
	Keyword::create_tables();
	UserState::create_table();
	Notification::create_table();

	if settings().database.backup {
		let backup_dir = data_dir.join("backup");

		start_backup_cycle(backup_dir);
	}
}

/// Convenience macro to make a blocking tokio task and await it, creating a
/// [`tracing`] span for the operation.
#[macro_export]
macro_rules! await_db {
	($name:literal: |$conn:ident| $body:block) => {{
		use ::anyhow::Context as _;

		let parent = ::tracing::Span::current();

		::tokio::task::spawn_blocking(move || -> ::anyhow::Result<_> {
			let span = ::tracing::info_span!(parent: &parent, "await_db");
			let _entered = span.enter();
			#[allow(unused_mut)]
			let mut $conn = $crate::db::connection();

			$body
		})
		.await
		.expect(concat!("Failed to join ", $name, " task"))
		.context(concat!("Failed to run DB query ", $name))
	}};
}

/// Convenience trait for converting IDs to and from `i64`, the integer type
/// SQLite supports.
trait IdI64Ext {
	fn into_i64(self) -> i64;

	fn from_i64(x: i64) -> Self;
}

macro_rules! impl_id_ext {
	($ty:ty $(, $($tys:ty),*)?) => {
		impl IdI64Ext for $ty {
			fn into_i64(self) -> i64 {
				self.0.try_into().unwrap()
			}

			fn from_i64(x: i64) -> Self {
				Self(x.try_into().unwrap())
			}
		}

		impl_id_ext!($($($tys),*)?);
	};
	() => {};
}

impl_id_ext!(UserId, ChannelId, GuildId, MessageId);
