// Copyright 2021 Benjamin Scherer
// Licensed under the Open Software License version 3.0

//! Interface for interacting with the sqlite database of keywords and other persistent user
//! information.

mod backup;
use backup::start_backup_cycle;

mod block;
mod ignore;
mod keyword;
mod mute;
mod notification;
mod opt_out;
mod user_state;

pub use block::Block;
pub use ignore::Ignore;
pub use keyword::{Keyword, KeywordKind};
pub use mute::Mute;
pub use notification::Notification;
pub use opt_out::OptOut;
pub use user_state::{UserState, UserStateKind};

use once_cell::sync::OnceCell;
use r2d2::{Pool, PooledConnection};
use r2d2_sqlite::SqliteConnectionManager;
use rusqlite::OpenFlags;

use std::{fs, io::ErrorKind};

use crate::global::settings;

/// Global connection pool to the database.
static POOL: OnceCell<Pool<SqliteConnectionManager>> = OnceCell::new();

/// Gets a connection from the global connection pool.
pub fn connection() -> PooledConnection<SqliteConnectionManager> {
	POOL.get()
		.expect("Database pool was not initialized")
		.get()
		.expect("Failed to obtain database connection")
}

/// Initializes the database.
///
/// Creates the data folder and database file if necessary, and starts backups if enabled.
pub fn init() {
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
/// [`Timer`](crate::monitoring::Timer) for performance monitoring.
#[macro_export]
macro_rules! await_db {
	($name:literal: |$conn:ident| $body:block) => {{
		use ::anyhow::Context as _;

		let _timer = $crate::monitoring::Timer::query($name);
		::tokio::task::spawn_blocking(move || -> ::anyhow::Result<_> {
			let $conn = $crate::db::connection();

			$body
		})
		.await
		.expect(concat!("Failed to join ", $name, " task"))
		.context(concat!("Failed to run DB query ", $name))
	}};
}
