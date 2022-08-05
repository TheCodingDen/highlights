// Copyright 2022 ThatsNoMoon
// Licensed under the Open Software License version 3.0

//! Handling for blocked users.

use anyhow::{Context as _, Result};
use futures_util::TryStreamExt;
use sea_orm::{
	entity::prelude::{
		DeriveActiveModelBehavior, DeriveColumn, DeriveEntityModel,
		DerivePrimaryKey, DeriveRelation, EntityTrait, EnumIter, IdenStatic,
		PrimaryKeyTrait,
	},
	ColumnTrait, Condition, IntoActiveModel, QueryFilter, QuerySelect,
};
use serenity::model::id::UserId;

use super::{connection, DbInt, IdDbExt};

#[derive(
	Clone, Debug, PartialEq, Eq, DeriveEntityModel, DeriveActiveModelBehavior,
)]
#[sea_orm(table_name = "blocks")]
pub struct Model {
	#[sea_orm(primary_key)]
	pub(crate) user_id: DbInt,
	#[sea_orm(primary_key)]
	pub(crate) blocked_id: DbInt,
}

#[derive(Debug, EnumIter, DeriveRelation)]
pub enum Relation {}

/// Represents a blocked user.
#[derive(Debug, Clone)]
pub(crate) struct Block {
	/// The user who blocked them.
	pub(crate) user_id: UserId,
	/// The user who was blocked.
	pub(crate) blocked_id: UserId,
}

impl Block {
	/// Fetches the list of blocks a user has added from the DB.
	#[tracing::instrument]
	pub(crate) async fn user_blocks(user_id: UserId) -> Result<Vec<Self>> {
		Entity::find()
			.filter(Column::UserId.eq(user_id.into_db()))
			.stream(connection())
			.await?
			.map_err(Into::into)
			.map_ok(Block::from)
			.try_collect()
			.await
	}

	/// Adds this blocked user to the DB.
	#[tracing::instrument]
	pub(crate) async fn insert(self) -> Result<()> {
		Entity::insert(Model::from(self).into_active_model())
			.exec(connection())
			.await?;

		Ok(())
	}

	/// Checks if this block exists in the DB.
	#[tracing::instrument]
	pub(crate) async fn exists(self) -> Result<bool> {
		let count = Entity::find()
			.select_only()
			.column_as(Column::UserId.count(), QueryAs::BlockCount)
			.filter(
				Condition::all()
					.add(Column::UserId.eq(self.user_id.into_db()))
					.add(Column::BlockedId.eq(self.blocked_id.into_db())),
			)
			.into_values::<i64, QueryAs>()
			.one(connection())
			.await?;

		let count = count.context("No count for blocks returned")?;
		Ok(count == 1)
	}

	/// Deletes this blocked user from the DB (making them not blocked anymore).
	#[tracing::instrument]
	pub(crate) async fn delete(self) -> Result<()> {
		Entity::delete(Model::from(self).into_active_model())
			.exec(connection())
			.await?;

		Ok(())
	}
}

#[derive(Clone, Copy, Debug, EnumIter, DeriveColumn)]
enum QueryAs {
	BlockCount,
}

impl From<Model> for Block {
	fn from(model: Model) -> Self {
		Self {
			user_id: UserId::from_db(model.user_id),
			blocked_id: UserId::from_db(model.blocked_id),
		}
	}
}

impl From<Block> for Model {
	fn from(mute: Block) -> Self {
		Self {
			user_id: mute.user_id.into_db(),
			blocked_id: mute.blocked_id.into_db(),
		}
	}
}
