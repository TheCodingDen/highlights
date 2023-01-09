use sea_orm::sea_query::{Alias, Index, Query};
use sea_orm_migration::prelude::{
	async_trait, ColumnDef, DbErr, DeriveMigrationName, MigrationTrait,
	SchemaManager, Table,
};

use crate::db::notification::{self, Column};

#[derive(DeriveMigrationName)]
pub(crate) struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
	async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
		let tmp_table = Alias::new("__migrated_sent_notifications");
		manager
			.create_table(
				Table::create()
					.table(tmp_table.clone())
					.col(
						ColumnDef::new(Column::UserId).big_integer().not_null(),
					)
					.col(
						ColumnDef::new(Column::OriginalMessage)
							.big_integer()
							.not_null(),
					)
					.col(
						ColumnDef::new(Column::NotificationMessage)
							.big_integer()
							.not_null(),
					)
					.col(ColumnDef::new(Column::Keyword).string().not_null())
					.primary_key(
						Index::create()
							.col(Column::NotificationMessage)
							.col(Column::Keyword),
					)
					.to_owned(),
			)
			.await?;

		manager
			.exec_stmt(
				Query::insert()
					.into_table(tmp_table.clone())
					.columns([
						Column::UserId,
						Column::OriginalMessage,
						Column::NotificationMessage,
						Column::Keyword,
					])
					.select_from(
						Query::select()
							.from(notification::Entity)
							.columns([
								Column::UserId,
								Column::OriginalMessage,
								Column::NotificationMessage,
								Column::Keyword,
							])
							.to_owned(),
					)
					.map_err(|e| DbErr::Query(e.to_string()))?
					.to_owned(),
			)
			.await?;

		manager
			.drop_table(Table::drop().table(notification::Entity).to_owned())
			.await?;

		manager
			.rename_table(
				Table::rename()
					.table(tmp_table, notification::Entity)
					.to_owned(),
			)
			.await
	}

	async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
		let tmp_table = Alias::new("__migrated_sent_notifications");
		manager
			.create_table(
				Table::create()
					.table(tmp_table.clone())
					.col(
						ColumnDef::new(Column::UserId).big_integer().not_null(),
					)
					.col(
						ColumnDef::new(Column::OriginalMessage)
							.big_integer()
							.not_null(),
					)
					.col(
						ColumnDef::new(Column::NotificationMessage)
							.big_integer()
							.not_null()
							.primary_key(),
					)
					.col(ColumnDef::new(Column::Keyword).string().not_null())
					.to_owned(),
			)
			.await?;

		manager
			.exec_stmt(
				Query::insert()
					.into_table(tmp_table.clone())
					.columns([
						Column::UserId,
						Column::OriginalMessage,
						Column::NotificationMessage,
						Column::Keyword,
					])
					.select_from(
						Query::select()
							.from(notification::Entity)
							.columns([
								Column::UserId,
								Column::OriginalMessage,
								Column::NotificationMessage,
								Column::Keyword,
							])
							.to_owned(),
					)
					.map_err(|e| DbErr::Query(e.to_string()))?
					.to_owned(),
			)
			.await?;

		manager
			.drop_table(Table::drop().table(notification::Entity).to_owned())
			.await?;

		manager
			.rename_table(
				Table::rename()
					.table(tmp_table, notification::Entity)
					.to_owned(),
			)
			.await
	}
}
