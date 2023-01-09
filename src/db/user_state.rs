// Copyright 2022 ThatsNoMoon
// Licensed under the Open Software License version 3.0

//! Handling for user states; whether or not the last notification DM was
//! successful.

use anyhow::{bail, Result};
use sea_orm::{
	entity::prelude::{
		DeriveActiveModelBehavior, DeriveEntityModel, DerivePrimaryKey,
		DeriveRelation, EntityTrait, EnumIter, PrimaryKeyTrait,
	},
	sea_query::OnConflict,
	IntoActiveModel,
};
use serenity::model::id::UserId;

use super::{connection, DbInt, IdDbExt};

#[derive(
	Clone, Debug, PartialEq, Eq, DeriveEntityModel, DeriveActiveModelBehavior,
)]
#[sea_orm(table_name = "user_states")]
pub struct Model {
	#[sea_orm(primary_key)]
	pub(crate) user_id: DbInt,
	pub(crate) state: u8,
}

#[derive(Debug, EnumIter, DeriveRelation)]
pub enum Relation {}

/// Description of a user's state.
#[derive(Debug, Clone)]
pub(crate) struct UserState {
	pub(crate) user_id: UserId,
	pub(crate) state: UserStateKind,
}

#[derive(Debug, Clone, Copy)]
#[repr(u8)]
pub(crate) enum UserStateKind {
	/// Indicates that the last DM sent to notify this user failed.
	CannotDm = 0,
}

impl UserState {
	const CANNOT_DM_STATE: u8 = UserStateKind::CannotDm as u8;

	/// Fetches the state of the user with the given ID from the DB.
	///
	/// Returns `None` if the user has no recorded state.
	#[tracing::instrument]
	pub(crate) async fn user_state(user_id: UserId) -> Result<Option<Self>> {
		Entity::find_by_id(user_id.into_db())
			.one(connection())
			.await?
			.map(Self::try_from)
			.transpose()
	}

	/// Sets the state of the user in the DB.
	#[tracing::instrument]
	pub(crate) async fn set(self) -> Result<()> {
		Entity::insert(Model::from(self).into_active_model())
			.on_conflict(
				OnConflict::column(Column::State)
					.update_column(Column::State)
					.to_owned(),
			)
			.exec(connection())
			.await?;

		Ok(())
	}

	/// Deletes this user state from the DB.
	#[tracing::instrument]
	pub(crate) async fn delete(self) -> Result<()> {
		Self::clear(self.user_id).await
	}

	/// Clears any state of the user with the given ID.
	#[tracing::instrument]
	pub(crate) async fn clear(user_id: UserId) -> Result<()> {
		Entity::delete_by_id(user_id.into_db())
			.exec(connection())
			.await?;

		Ok(())
	}
}

impl TryFrom<Model> for UserState {
	type Error = anyhow::Error;

	fn try_from(model: Model) -> Result<Self> {
		Ok(Self {
			user_id: UserId::from_db(model.user_id),
			state: match model.state {
				Self::CANNOT_DM_STATE => UserStateKind::CannotDm,
				other => bail!("Unknown user state: {other}"),
			},
		})
	}
}

impl From<UserState> for Model {
	fn from(state: UserState) -> Self {
		Model {
			user_id: state.user_id.into_db(),
			state: state.state as u8,
		}
	}
}
