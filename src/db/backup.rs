// Copyright 2022 ThatsNoMoon
// Licensed under the Open Software License version 3.0

//! Automatic backup system.

use chrono::{DateTime, Duration, Utc};
use rusqlite::{backup::Backup, Connection, Error, OpenFlags};
use tokio::{fs, task, time::interval};

use std::{
	io::{Error as IoError, ErrorKind},
	path::{Path, PathBuf},
	time::Duration as StdDuration,
};

use super::connection;

/// Format used for backup timestamps. Can't use ISO-8601 because windows
/// doesn't seem to allow file names to contain `:`.
const TIMESTAMP_FORMAT: &str = "%Y-%m-%dT%H_%M_%S%.f%z";

/// Creates the dir at the specified path for backups.
///
/// Returns `Ok(())` on success or when the directory already existed.
#[tracing::instrument]
async fn ensure_backup_dir_exists(path: &Path) -> Result<(), IoError> {
	let result = fs::create_dir(path).await;
	if let Err(error) = &result {
		if error.kind() == ErrorKind::AlreadyExists {
			return Ok(());
		}
	}
	result
}

/// Creates a backup in the specified directory.
#[tracing::instrument]
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

		backup.run_to_completion(5, StdDuration::from_millis(250), None)
	})
	.await
	.expect("Failed to join backup task")
}

/// Cleans up old backups from the specified directory.
#[tracing::instrument]
async fn clean_backups(backup_dir: &Path) {
	#[derive(Default)]
	struct Backups {
		files: Vec<(PathBuf, DateTime<Utc>)>,
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

			self.files.push((path, backup_time));
			Ok(())
		}

		async fn clean(mut self) -> Vec<Result<(), IoError>> {
			if self.files.len() <= 1 {
				return vec![];
			}

			let mut results = Vec::new();

			// sort by most recent first
			self.files
				.sort_unstable_by(|(_, time1), (_, time2)| time2.cmp(time1));

			let mut last_time = self.files.remove(0).1;
			let now = Utc::now();

			let mut daily_found = 0;
			let mut weekly_found = 0;
			let mut monthly_found = 0;

			for (path, time) in self.files {
				if now - time < Duration::days(1) {
					continue;
				}

				let gap = last_time - time;

				if daily_found < 7 {
					// includes some wiggle room so backups made 23.99999 hours
					// apart aren't deleted
					if gap < Duration::days(1) - Duration::minutes(1) {
						tracing::debug!(
							"Deleting old restart backup from {}",
							time.date()
						);
						results.push(fs::remove_file(path).await);
					} else {
						last_time = time;
						daily_found += 1;
					}
				} else if weekly_found < 4 {
					if gap < Duration::weeks(1) - Duration::minutes(10) {
						tracing::debug!(
							"Deleting old daily backup from {}",
							time.date()
						);
						results.push(fs::remove_file(path).await);
					} else {
						last_time = time;
						weekly_found += 1;
					}
				} else if monthly_found < 12 {
					if gap < Duration::days(30) - Duration::minutes(30) {
						tracing::debug!(
							"Deleting old weekly backup from {}",
							time.date()
						);
						results.push(fs::remove_file(path).await);
					} else {
						last_time = time;
						monthly_found += 1;
					}
				} else if gap < Duration::days(364) {
					tracing::debug!(
						"Deleting old monthly backup from {}",
						time.date()
					);
					results.push(fs::remove_file(path).await);
				} else {
					last_time = time;
				}
			}

			results
		}
	}

	let mut backups = Backups::default();

	let mut dir = match fs::read_dir(&backup_dir).await {
		Ok(dir) => dir,
		Err(e) => {
			tracing::error!(
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
					tracing::warn!("Invalid backup name: {:?}", path);
				}
			}
			Ok(None) => break,
			Err(e) => {
				tracing::error!(
					"Error reading backup directory for cleaning: {0}\n{0:?}",
					e
				);
				break;
			}
		}
	}

	for result in backups.clean().await {
		if let Err(e) = result {
			tracing::error!("Error cleaning backup: {0}\n{0:?}", e);
		}
	}
}

/// Starts the automatic backup cycle.
///
/// Creates `<data directory>/backup` if it doesn't exist already, creates a backup, cleans up old
/// backups, and repeats once every 24hrs.
pub(crate) fn start_backup_cycle(backup_dir: PathBuf) {
	let _ = ensure_backup_dir_exists(&backup_dir);

	task::spawn(async move {
		let mut daily = interval(StdDuration::from_secs(60 * 60 * 24));

		loop {
			daily.tick().await;

			tracing::info!("Backing up database...");
			if let Err(error) = ensure_backup_dir_exists(&backup_dir).await {
				tracing::error!(
					"Failed to create backup directory: {0}\n{0:?}",
					error
				);
				continue;
			}

			tracing::info!("Cleaning up old backups...");
			if let Err(error) = create_backup(backup_dir.clone()).await {
				tracing::error!("Error backing up database: {0}\n{0:?}", error);
			}

			clean_backups(&backup_dir).await;
		}
	});
}
