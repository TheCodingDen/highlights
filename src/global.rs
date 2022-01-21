// Copyright 2022 ThatsNoMoon
// Licensed under the Open Software License version 3.0

//! Global constants, both compile-time and set at runtime.

/// How many times to retry notifications after internal server errors from Discord.
pub(crate) const NOTIFICATION_RETRIES: u8 = 5;

/// Color of normal embeds (from help command and notifications).
pub(crate) const EMBED_COLOR: u32 = 0xefff47;
pub(crate) const ERROR_COLOR: u32 = 0xff4747;
