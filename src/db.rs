// Copyright 2020 Benjamin Scherer
// Licensed under the Open Software License version 3.0

mod backup;
use backup::start_backup_cycle;

mod block;
mod ignore;
mod keyword;
mod mute;
mod user_state;

pub use block::Block;
pub use ignore::Ignore;
pub use keyword::{Keyword, KeywordKind};
pub use mute::Mute;
pub use user_state::{UserState, UserStateKind};

use once_cell::sync::OnceCell;
use r2d2::{Pool, PooledConnection};
use r2d2_sqlite::SqliteConnectionManager;
use rusqlite::OpenFlags;

use std::{env, fs, io::ErrorKind, path::PathBuf};

static POOL: OnceCell<Pool<SqliteConnectionManager>> = OnceCell::new();

pub fn connection() -> PooledConnection<SqliteConnectionManager> {
	POOL.get()
		.expect("Database pool was not initialized")
		.get()
		.expect("Failed to obtain database connection")
}

pub fn init() {
	let data_dir: PathBuf = env::var("HIGHLIGHTS_DATA_DIR")
		.map(Into::into)
		.unwrap_or("data".into());

	if let Err(error) = fs::create_dir(&data_dir) {
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
	Keyword::create_tables();
	UserState::create_table();

	if env::var_os("HIGHLIGHTS_DONT_BACKUP").is_none() {
		let backup_dir = data_dir.join("backup");

		start_backup_cycle(backup_dir);
	}
}

#[macro_export]
macro_rules! await_db {
	($name:literal: |$conn:ident| $body:block) => {{
		let _timer = $crate::monitoring::Timer::query($name);
		::tokio::task::spawn_blocking(move || {
			let $conn = $crate::db::connection();

			$body
			})
		.await
		.expect(concat!("Failed to join ", $name, " task"))
		}};
}
