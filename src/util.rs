use automate::{
	gateway::{Channel, Guild, Overwrite, OverwriteType, Permission},
	Snowflake,
};
use smallvec::SmallVec;
use tokio::task::spawn_blocking;

pub fn member_can_read_channel(
	user_id: Snowflake,
	member_roles: &[Snowflake],
	channel: &Channel,
	guild: &Guild,
) -> bool {
	if guild.owner_id == user_id {
		return true;
	}

	let roles = guild
		.roles
		.iter()
		.filter(|role| member_roles.contains(&role.id))
		.collect::<SmallVec<[_; 8]>>();

	let permissions = roles
		.iter()
		.fold(0, |perms, role| (perms | role.permissions));

	if permissions & Permission::Administrator as u32 != 0 {
		return true;
	}

	let permissions = channel
		.permission_overwrites
		.as_ref()
		.into_iter()
		.flatten()
		.fold(permissions, |mut perms, overwrite| {
			let apply = matches!(
				overwrite,
				Overwrite { id, _type: OverwriteType::Role, .. }
					if roles.iter().any(|role| role.id == *id)
			) || matches!(
				overwrite,
				Overwrite { id, _type: OverwriteType::Member, .. }
					if user_id == *id
			);

			if apply {
				perms &= !overwrite.deny;
				perms |= overwrite.allow;
			}

			perms
		});

	permissions & Permission::ViewChannel as u32 != 0
}

pub async fn spawn_blocking_expect<F, R>(task: F) -> R
where
	F: FnOnce() -> R + Send + 'static,
	R: Send + 'static,
{
	spawn_blocking(task).await.expect("Failed to join task")
}
