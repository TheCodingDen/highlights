use sea_orm_migration::{MigrationTrait, MigratorTrait};

mod m2022_08_04_000001_init;
mod m2023_01_08_000001_composite_notification_key;
mod m2023_05_18_000001_rename_pkey_index;

pub(crate) struct Migrator;

impl MigratorTrait for Migrator {
	fn migrations() -> Vec<Box<dyn MigrationTrait>> {
		vec![
			Box::new(m2022_08_04_000001_init::Migration),
			Box::new(m2023_01_08_000001_composite_notification_key::Migration),
			Box::new(m2023_05_18_000001_rename_pkey_index::Migration),
		]
	}
}
