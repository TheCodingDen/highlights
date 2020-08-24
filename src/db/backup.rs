// Copyright 2020 Benjamin Scherer
// Licensed under the Open Software License version 3.0

use chrono::{DateTime, Utc};
use rusqlite::{backup::Backup, Connection, Error, OpenFlags};
use tokio::{fs, task, time::interval};

use std::{
	io::{Error as IoError, ErrorKind},
	path::{Path, PathBuf},
	time::Duration,
};

use super::connection;

/// Format used for backup timestamps. Can't use ISO-8601 because windows
/// doesn't seem to allow file names to contain `:`.
const TIMESTAMP_FORMAT: &str = "%Y-%m-%dT%H_%M_%S%.f%z";

async fn ensure_backup_dir_exists(path: &Path) -> Result<(), IoError> {
	let result = fs::create_dir(path).await;
	if let Err(error) = &result {
		if error.kind() == ErrorKind::AlreadyExists {
			return Ok(());
		}
	}
	result
}

async fn create_backup(backup_dir: PathBuf) -> Result<(), Error> {
	task::spawn_blocking(move || {
		let conn = connection();

		let backup_name = format!(
			"{}_data_backup_{}.db",
			env!("CARGO_PKG_NAME"),
			Utc::now().format(TIMESTAMP_FORMAT)
		);

		let backup_path = backup_dir.join(backup_name);

		let mut output_conn = Connection::open_with_flags(
			backup_path,
			OpenFlags::SQLITE_OPEN_CREATE | OpenFlags::SQLITE_OPEN_READ_WRITE,
		)?;

		let backup = Backup::new(&conn, &mut output_conn)?;

		backup.run_to_completion(5, Duration::from_millis(250), None)
	})
	.await
	.expect("Failed to join backup task")
}

async fn clean_backups(backup_dir: &Path) {
	#[derive(Default)]
	struct Backups {
		old: Vec<(PathBuf, DateTime<Utc>)>,
		monthly: Vec<(PathBuf, DateTime<Utc>)>,
		weekly: Vec<(PathBuf, DateTime<Utc>)>,
		daily: Vec<(PathBuf, DateTime<Utc>)>,
	}

	impl Backups {
		fn add(&mut self, path: PathBuf) -> Result<(), PathBuf> {
			let backup_name =
				match path.file_name().and_then(|name| name.to_str()) {
					Some(name) => name,
					None => return Err(path),
				};

			let backup_prefix =
				concat!(env!("CARGO_PKG_NAME"), "_data_backup_");

			let backup_time: DateTime<Utc> = match backup_name
				.strip_prefix(backup_prefix)
				.and_then(|s| s.strip_suffix(".db"))
				.and_then(|date_str| {
					DateTime::parse_from_str(date_str, TIMESTAMP_FORMAT).ok()
				}) {
				Some(date) => date.into(),
				None => return Err(path),
			};

			let elapsed = Utc::now() - backup_time;
			let days = elapsed.num_days();

			let bucket = if days >= 365 {
				&mut self.old
			} else if days >= 30 {
				&mut self.monthly
			} else if days >= 7 {
				&mut self.weekly
			} else if days >= 1 {
				&mut self.daily
			} else {
				return Ok(());
			};

			bucket.push((path, backup_time));
			Ok(())
		}

		async fn clean(&mut self) -> Vec<Result<(), IoError>> {
			let mut results = Vec::new();

			for (old, _) in self.old.drain(..) {
				results.push(fs::remove_file(old).await);
			}

			if self.monthly.len() > 12 {
				self.monthly.sort_unstable_by(|(_, time1), (_, time2)| {
					time2.cmp(time1)
				});

				let to_delete = self.monthly.len() - 12;

				for (monthly, _) in self.monthly.drain(..to_delete) {
					results.push(fs::remove_file(monthly).await);
				}
			}

			if self.weekly.len() > 4 {
				self.weekly.sort_unstable_by(|(_, time1), (_, time2)| {
					time2.cmp(time1)
				});

				let to_delete = self.weekly.len() - 4;

				for (weekly, _) in self.weekly.drain(..to_delete) {
					results.push(fs::remove_file(weekly).await);
				}
			}

			if self.daily.len() > 7 {
				self.daily.sort_unstable_by(|(_, time1), (_, time2)| {
					time2.cmp(time1)
				});

				let to_delete = self.daily.len() - 7;

				for (daily, _) in self.daily.drain(..to_delete) {
					results.push(fs::remove_file(daily).await);
				}
			}

			results
		}
	}

	let mut backups = Backups::default();

	let mut dir = match fs::read_dir(&backup_dir).await {
		Ok(dir) => dir,
		Err(e) => {
			log::error!(
				"Error reading backup directory for cleaning: {0}\n{0:?}",
				e
			);
			return;
		}
	};

	loop {
		match dir.next_entry().await {
			Ok(Some(dir)) => {
				if let Err(path) = backups.add(dir.path()) {
					log::warn!("Invalid backup name: {:?}", path);
				}
			}
			Ok(None) => break,
			Err(e) => {
				log::error!(
					"Error reading backup directory for cleaning: {0}\n{0:?}",
					e
				);
				break;
			}
		}
	}

	for result in backups.clean().await {
		if let Err(e) = result {
			log::error!("Error cleaning backup: {0}\n{0:?}", e);
		}
	}
}

pub fn start_backup_cycle(backup_dir: PathBuf) {
	let _ = ensure_backup_dir_exists(&backup_dir);

	task::spawn(async move {
		let mut daily = interval(Duration::from_secs(60 * 60 * 24));

		loop {
			daily.tick().await;

			log::info!("Backing up database...");
			if let Err(error) = ensure_backup_dir_exists(&backup_dir).await {
				log::error!(
					"Failed to create backup directory: {0}\n{0:?}",
					error
				);
				continue;
			}

			log::info!("Cleaning up old backups...");
			if let Err(error) = create_backup(backup_dir.clone()).await {
				log::error!("Error backing up database: {0}\n{0:?}", error);
			}

			clean_backups(&backup_dir).await;
		}
	});
}
