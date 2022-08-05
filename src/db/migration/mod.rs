use sea_orm_migration::{MigrationTrait, MigratorTrait};

mod m2022_08_04_000001_init;

pub(crate) struct Migrator;

impl MigratorTrait for Migrator {
	fn migrations() -> Vec<Box<dyn MigrationTrait>> {
		vec![Box::new(m2022_08_04_000001_init::Migration)]
	}
}
