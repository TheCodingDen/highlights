// Copyright 2022 ThatsNoMoon
// Licensed under the Open Software License version 3.0

//! Handling for mutes.

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
use serenity::model::id::{ChannelId, UserId};

use super::{connection, DbInt, IdDbExt};

#[derive(
	Clone, Debug, PartialEq, Eq, DeriveEntityModel, DeriveActiveModelBehavior,
)]
#[sea_orm(table_name = "mutes")]
pub struct Model {
	#[sea_orm(primary_key)]
	pub(crate) user_id: DbInt,
	#[sea_orm(primary_key)]
	pub(crate) channel_id: DbInt,
}

#[derive(Debug, EnumIter, DeriveRelation)]
pub enum Relation {}

/// Represents a muted channel.
#[derive(Debug, Clone)]
pub(crate) struct Mute {
	/// The ID of the user who muted the channel.
	pub(crate) user_id: UserId,
	/// The ID of the channel that was muted.
	pub(crate) channel_id: ChannelId,
}

impl Mute {
	/// Fetches a list of mutes for the user with the given ID from the DB.
	#[tracing::instrument]
	pub(crate) async fn user_mutes(user_id: UserId) -> Result<Vec<Mute>> {
		Entity::find()
			.filter(Column::UserId.eq(user_id.into_db()))
			.stream(connection())
			.await?
			.map_err(Into::into)
			.map_ok(Mute::from)
			.try_collect()
			.await
	}

	/// Checks if this mute exists in the DB.
	///
	/// Returns true if a mute with `self.user_id` and `self.channel_id` exists
	/// in the DB.
	#[tracing::instrument]
	pub(crate) async fn exists(self) -> Result<bool> {
		let count = Entity::find()
			.select_only()
			.column_as(Column::UserId.count(), QueryAs::MuteCount)
			.filter(
				Condition::all()
					.add(Column::UserId.eq(self.user_id.into_db()))
					.add(Column::ChannelId.eq(self.channel_id.into_db())),
			)
			.into_values::<u32, QueryAs>()
			.one(connection())
			.await?;

		let count = count.context("No count for mutes returned")?;
		Ok(count == 1)
	}

	/// Inserts this mute into the DB.
	#[tracing::instrument]
	pub(crate) async fn insert(self) -> Result<()> {
		Entity::insert(Model::from(self).into_active_model())
			.exec(connection())
			.await?;

		Ok(())
	}

	/// Deletes this mute from the DB.
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
	MuteCount,
}

impl From<Model> for Mute {
	fn from(model: Model) -> Self {
		Self {
			user_id: UserId::from_db(model.user_id),
			channel_id: ChannelId::from_db(model.channel_id),
		}
	}
}

impl From<Mute> for Model {
	fn from(mute: Mute) -> Self {
		Self {
			user_id: mute.user_id.into_db(),
			channel_id: mute.channel_id.into_db(),
		}
	}
}
