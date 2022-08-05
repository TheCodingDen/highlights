// Copyright 2022 ThatsNoMoon
// Licensed under the Open Software License version 3.0

//! Interface for interacting with the sqlite database of keywords and other persistent user
//! information.

#![cfg_attr(not(feature = "bot"), allow(dead_code))]

#[cfg(not(any(feature = "sqlite", feature = "postgresql")))]
compile_error!("The sqlite feature or the postgresql feature must be enabled");

#[cfg(feature = "backup")]
mod backup;
mod block;
mod channel_keyword;
mod guild_keyword;
mod ignore;
mod keyword;
mod migration;
mod mute;
mod notification;
mod opt_out;
mod user_state;

use anyhow::{anyhow, Result};
use once_cell::sync::OnceCell;
use sea_orm::{Database, DatabaseConnection};
use sea_orm_migration::MigratorTrait;
use serenity::model::id::{ChannelId, GuildId, MessageId, UserId};

use self::migration::Migrator;
#[cfg(feature = "bot")]
pub(crate) use self::{
	block::Block,
	ignore::Ignore,
	keyword::{Keyword, KeywordKind},
	mute::Mute,
	notification::Notification,
	opt_out::OptOut,
	user_state::{UserState, UserStateKind},
};
use crate::settings::settings;

/// Global connection pool to the database.
static CONNECTION: OnceCell<DatabaseConnection> = OnceCell::new();

/// Gets a connection from the global connection pool.
#[tracing::instrument]
pub(crate) fn connection() -> &'static DatabaseConnection {
	CONNECTION
		.get()
		.expect("Database connection was not initialized")
}

/// Initializes the database.
///
/// Creates the data folder and database file if necessary, and starts backups
/// if enabled.
pub(crate) async fn init() -> Result<()> {
	#[cfg(feature = "sqlite")]
	{
		use std::{fs::create_dir, io::ErrorKind};

		use anyhow::{bail, Context as _};

		let (path, url) = {
			let s = settings();
			(s.database.path.as_ref(), s.database.url.as_ref())
		};

		match (path, url) {
			(Some(data_dir), None) => {
				if let Err(error) = create_dir(data_dir) {
					if error.kind() != ErrorKind::AlreadyExists {
						Err::<(), _>(error)
							.context("Failed to create data directory")?;
					}
				}

				let db_path = data_dir.join("data.db");

				init_connection(format!("sqlite://{}", db_path.display()))
					.await?;

				#[cfg(feature = "backup")]
				if settings().database.backup != Some(false) {
					let backup_dir = data_dir.join("backup");

					backup::start_backup_cycle(db_path, backup_dir);
				}
			}
			(None, Some(url)) => init_connection(url.to_string()).await?,
			(None, None) => {
				bail!("One of database.path and database.url must be set")
			}
			(Some(_), Some(_)) => {
				bail!("Only one of database.path and database.url can be set")
			}
		}
	}

	#[cfg(not(feature = "sqlite"))]
	init_connection(settings().database.url.to_string()).await?;

	Migrator::up(connection(), None).await?;

	Ok(())
}

async fn init_connection(url: String) -> Result<()> {
	let conn = Database::connect(url).await?;

	CONNECTION
		.set(conn)
		.map_err(|_| anyhow!("Database connection already initialized"))?;

	Ok(())
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
			let mut $conn = $crate::db::pool();

			$body
		})
		.await
		.expect(concat!("Failed to join ", $name, " task"))
		.context(concat!("Failed to run DB query ", $name))
	}};
}

type DbInt = i64;

/// Convenience trait for converting IDs to and from `DbInt`.
trait IdDbExt {
	fn into_db(self) -> DbInt;

	fn from_db(x: DbInt) -> Self;
}

macro_rules! impl_id_ext {
	($ty:ty $(, $($tys:ty),*)?) => {
		impl IdDbExt for $ty {
			fn into_db(self) -> DbInt {
				self.0.try_into().unwrap()
			}

			fn from_db(x: DbInt) -> Self {
				Self(x.try_into().unwrap())
			}
		}

		impl_id_ext!($($($tys),*)?);
	};
	() => {};
}

impl_id_ext!(UserId, ChannelId, GuildId, MessageId);
