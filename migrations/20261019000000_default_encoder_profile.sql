-- SPDX-License-Identifier: AGPL-3.0-or-later
--
-- M2: the default encoder profile of a fresh install, so a new station only
-- has to enter its server details. See docs/outputs.md#default-profiles.
-- `AAC 96k` is seeded with the AAC encoder in M10.
INSERT IGNORE INTO encoder_profiles (name, encoder, bitrate_kbps, sample_rate, channels, options)
VALUES ('MP3 128k', 'mp3', 128, 44100, 2, JSON_OBJECT('mode', 'cbr'));
