// Copyright 2022 ThatsNoMoon
// Licensed under the Open Software License version 3.0

use sea_orm::entity::prelude::{
	DeriveActiveModelBehavior, DeriveEntityModel, DerivePrimaryKey,
	DeriveRelation, EntityTrait, EnumIter, IdenStatic, PrimaryKeyTrait,
};

use super::DbInt;

#[derive(
	Clone, Debug, PartialEq, Eq, DeriveEntityModel, DeriveActiveModelBehavior,
)]
#[sea_orm(table_name = "guild_keywords")]
pub struct Model {
	#[sea_orm(primary_key)]
	pub(crate) keyword: String,
	#[sea_orm(primary_key)]
	pub(crate) user_id: DbInt,
	#[sea_orm(primary_key)]
	pub(crate) guild_id: DbInt,
}

#[derive(Debug, EnumIter, DeriveRelation)]
pub enum Relation {}
