// Copyright 2022 ThatsNoMoon
// Licensed under the Open Software License version 3.0

//! Handling for user opt-outs.

use anyhow::Result;
use futures_util::FutureExt;
use sea_orm::{
	entity::prelude::{
		DeriveActiveModelBehavior, DeriveEntityModel, DerivePrimaryKey,
		DeriveRelation, EntityTrait, EnumIter, PrimaryKeyTrait,
	},
	ActiveValue, ColumnTrait, DbErr, QueryFilter, TransactionTrait,
};
use serenity::model::id::UserId;

use super::{
	block, channel_keyword, connection, guild_keyword, ignore, mute, DbInt,
	IdDbExt,
};

#[derive(
	Clone, Debug, PartialEq, Eq, DeriveEntityModel, DeriveActiveModelBehavior,
)]
#[sea_orm(table_name = "opt_outs")]
pub struct Model {
	#[sea_orm(primary_key)]
	pub(crate) user_id: DbInt,
}

#[derive(Debug, EnumIter, DeriveRelation)]
pub enum Relation {}

/// Represents an opt-out made by a user.
///
/// Users that opt-out will not have their messages highlighted.
#[derive(Debug, Clone)]
pub(crate) struct OptOut {
	/// The user that opted out.
	pub(crate) user_id: UserId,
}

impl OptOut {
	/// Checks if this opt-out already exists in the DB.
	#[tracing::instrument]
	pub(crate) async fn exists(self) -> Result<bool> {
		let result = Entity::find_by_id(self.user_id.into_db())
			.one(connection())
			.await?;

		Ok(result.is_some())
	}

	/// Adds this opt-out to the DB.
	#[tracing::instrument]
	pub(crate) async fn insert(self) -> Result<()> {
		Entity::insert(ActiveModel {
			user_id: ActiveValue::Set(self.user_id.into_db()),
		})
		.exec(connection())
		.await?;

		Ok(())
	}

	/// Deletes this opt-out from the DB.
	#[tracing::instrument]
	pub(crate) async fn delete(self) -> Result<()> {
		Entity::delete(ActiveModel {
			user_id: ActiveValue::Set(self.user_id.into_db()),
		})
		.exec(connection())
		.await?;

		Ok(())
	}

	/// Deletes this user's data from the DB as they opt out.
	#[tracing::instrument]
	pub(crate) async fn delete_user_data(self) -> Result<()> {
		let user_id = self.user_id.into_db();

		connection()
			.transaction(|transaction| {
				async move {
					guild_keyword::Entity::delete_many()
						.filter(guild_keyword::Column::UserId.eq(user_id))
						.exec(transaction)
						.await?;

					channel_keyword::Entity::delete_many()
						.filter(channel_keyword::Column::UserId.eq(user_id))
						.exec(transaction)
						.await?;

					block::Entity::delete_many()
						.filter(block::Column::UserId.eq(user_id))
						.exec(transaction)
						.await?;

					mute::Entity::delete_many()
						.filter(mute::Column::UserId.eq(user_id))
						.exec(transaction)
						.await?;

					ignore::Entity::delete_many()
						.filter(ignore::Column::UserId.eq(user_id))
						.exec(transaction)
						.await?;

					Ok::<(), DbErr>(())
				}
				.boxed()
			})
			.await
			.map_err(Into::into)
	}
}
