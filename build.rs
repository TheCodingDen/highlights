use sqlx::{query, sqlite::SqliteConnectOptions, ConnectOptions};

use std::{env, str::FromStr};

#[tokio::main]
async fn main() {
	let mut conn = SqliteConnectOptions::from_str(
		&env::var("DATABASE_URL").unwrap_or(String::from("sqlite://./data.db")),
	)
	.expect("Failed to parse connection options")
	.create_if_missing(true)
	.connect()
	.await
	.expect("Failed to open database connection");

	query("CREATE TABLE IF NOT EXISTS follows (user_id INTEGER NOT NULL, channel_id INTEGER NOT NULL, PRIMARY KEY (user_id, channel_id))")
		.execute(&mut conn)
		.await
		.expect("Failed to create highlights_follows table");

	query("CREATE TABLE IF NOT EXISTS keywords (keyword TEXT NOT NULL, user_id INTEGER NOT NULL, server_id INTEGER NOT NULL, PRIMARY KEY (keyword, user_id, server_id))")
		.execute(&mut conn)
		.await
		.expect("Failed to create highlights_keywords table");
}
