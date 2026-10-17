// SPDX-License-Identifier: AGPL-3.0-or-later
//
// Generated from openapi.json by `npm run generate-api`. Do not edit.

export interface paths {
    "/api/v1/auth/login": {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        get?: never;
        put?: never;
        /** Signs in: answers with the user and sets the `heron_session` cookie. */
        post: operations["login"];
        delete?: never;
        options?: never;
        head?: never;
        patch?: never;
        trace?: never;
    };
    "/api/v1/auth/logout": {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        get?: never;
        put?: never;
        /** Signs out: ends the session and clears the cookie. */
        post: operations["logout"];
        delete?: never;
        options?: never;
        head?: never;
        patch?: never;
        trace?: never;
    };
    "/api/v1/auth/me": {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        /** The signed-in user. */
        get: operations["me"];
        put?: never;
        post?: never;
        delete?: never;
        options?: never;
        head?: never;
        patch?: never;
        trace?: never;
    };
    "/api/v1/auth/password": {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        get?: never;
        /** Changes your password. Your other sessions are signed out. */
        put: operations["change_password"];
        post?: never;
        delete?: never;
        options?: never;
        head?: never;
        patch?: never;
        trace?: never;
    };
    "/api/v1/auth/preferences": {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        get?: never;
        /** Saves your preferences: the web UI's theme. */
        put: operations["set_preferences"];
        post?: never;
        delete?: never;
        options?: never;
        head?: never;
        patch?: never;
        trace?: never;
    };
    "/api/v1/categories": {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        get: operations["list_categories"];
        put?: never;
        /** Operators and admins. */
        post: operations["create_category"];
        delete?: never;
        options?: never;
        head?: never;
        patch?: never;
        trace?: never;
    };
    "/api/v1/categories/{id}": {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        get: operations["get_category"];
        /** Renames or recolors a category. Operators and admins. */
        put: operations["update_category"];
        post?: never;
        /**
         * Deletes a category; its tracks stay in the library. Refused while a
         *     clock uses it. Operators and admins.
         */
        delete: operations["delete_category"];
        options?: never;
        head?: never;
        patch?: never;
        trace?: never;
    };
    "/api/v1/encoder-profiles": {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        get: operations["list_profiles"];
        put?: never;
        /** Adds an encoder profile. Admins only. */
        post: operations["create_profile"];
        delete?: never;
        options?: never;
        head?: never;
        patch?: never;
        trace?: never;
    };
    "/api/v1/encoder-profiles/{id}": {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        get: operations["get_profile"];
        /**
         * Replaces an encoder profile. Outputs using it restart with the new
         *     encoding. Admins only.
         */
        put: operations["update_profile"];
        post?: never;
        /** Deletes an encoder profile no output uses. Admins only. */
        delete: operations["delete_profile"];
        options?: never;
        head?: never;
        patch?: never;
        trace?: never;
    };
    "/api/v1/engine": {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        get: operations["engine_status"];
        put?: never;
        post?: never;
        delete?: never;
        options?: never;
        head?: never;
        patch?: never;
        trace?: never;
    };
    "/api/v1/history": {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        /** Plays, most recent first. */
        get: operations["list_history"];
        put?: never;
        post?: never;
        delete?: never;
        options?: never;
        head?: never;
        patch?: never;
        trace?: never;
    };
    "/api/v1/library/batches": {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        /** The most recent batches of file operations, newest first. */
        get: operations["list_batches"];
        put?: never;
        post?: never;
        delete?: never;
        options?: never;
        head?: never;
        patch?: never;
        trace?: never;
    };
    "/api/v1/library/batches/{id}/undo": {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        get?: never;
        put?: never;
        /**
         * Reverts a batch: files go back to their previous paths and get their
         *     previous tags back. Nothing is overwritten: a file whose previous path
         *     is taken stays where it is (see the task's report). Admins.
         */
        post: operations["undo_batch"];
        delete?: never;
        options?: never;
        head?: never;
        patch?: never;
        trace?: never;
    };
    "/api/v1/library/organize": {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        get?: never;
        put?: never;
        /**
         * Asks for a dry run of organizing the library with the layout in the
         *     library settings: the task's `plan` lists every move, nothing changes.
         *     Admins.
         */
        post: operations["plan_organization"];
        delete?: never;
        options?: never;
        head?: never;
        patch?: never;
        trace?: never;
    };
    "/api/v1/library/organize/{plan_task_id}/apply": {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        get?: never;
        put?: never;
        /**
         * Carries out the moves of a finished dry run, as one batch that can be
         *     undone. Tracks moved since the dry run are skipped. Admins.
         */
        post: operations["apply_organization"];
        delete?: never;
        options?: never;
        head?: never;
        patch?: never;
        trace?: never;
    };
    "/api/v1/library/review": {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        /**
         * Tracks whose identification was not confident enough to apply, with the
         *     releases they may be on.
         */
        get: operations["list_review"];
        put?: never;
        post?: never;
        delete?: never;
        options?: never;
        head?: never;
        patch?: never;
        trace?: never;
    };
    "/api/v1/library/status": {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        /** Import and analysis progress. No sign-in needed. */
        get: operations["library_status"];
        put?: never;
        post?: never;
        delete?: never;
        options?: never;
        head?: never;
        patch?: never;
        trace?: never;
    };
    "/api/v1/library/tasks": {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        /** The most recent library tasks, newest first. */
        get: operations["list_tasks"];
        put?: never;
        post?: never;
        delete?: never;
        options?: never;
        head?: never;
        patch?: never;
        trace?: never;
    };
    "/api/v1/library/tasks/{id}": {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        /** A library task, with its result once done. */
        get: operations["get_task"];
        put?: never;
        post?: never;
        delete?: never;
        options?: never;
        head?: never;
        patch?: never;
        trace?: never;
    };
    "/api/v1/live": {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        /** Opens the live WebSocket. Messages are [`LiveMessage`]s. */
        get: operations["live"];
        put?: never;
        post?: never;
        delete?: never;
        options?: never;
        head?: never;
        patch?: never;
        trace?: never;
    };
    "/api/v1/outputs": {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        /** Outputs with their live state. */
        get: operations["list_outputs"];
        put?: never;
        /**
         * Adds an output; the engine starts streaming to it if it is enabled.
         *     Admins only.
         */
        post: operations["create_output"];
        delete?: never;
        options?: never;
        head?: never;
        patch?: never;
        trace?: never;
    };
    "/api/v1/outputs/{id}": {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        get: operations["get_output"];
        /** Replaces an output. Credentials left out of `config` are kept. Admins only. */
        put: operations["update_output"];
        post?: never;
        /** Deletes an output; the engine stops streaming to it. Admins only. */
        delete: operations["delete_output"];
        options?: never;
        head?: never;
        patch?: never;
        trace?: never;
    };
    "/api/v1/player": {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        /** Decks, queue and outputs, read from the engine now. */
        get: operations["get_player"];
        put?: never;
        post?: never;
        delete?: never;
        options?: never;
        head?: never;
        patch?: never;
        trace?: never;
    };
    "/api/v1/player/fx": {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        get?: never;
        put?: never;
        /**
         * Plays a library track on the FX player, over the music. Replaces the
         *     FX playing, if any.
         */
        post: operations["play_fx"];
        /** Fades out the FX player. */
        delete: operations["stop_fx"];
        options?: never;
        head?: never;
        patch?: never;
        trace?: never;
    };
    "/api/v1/player/levels": {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        /** Peak and RMS levels (dBFS) of the master bus and each deck. */
        get: operations["get_levels"];
        put?: never;
        post?: never;
        delete?: never;
        options?: never;
        head?: never;
        patch?: never;
        trace?: never;
    };
    "/api/v1/player/pause": {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        get?: never;
        put?: never;
        /** Fades out and holds the decks; the stream goes on with silence. */
        post: operations["pause"];
        delete?: never;
        options?: never;
        head?: never;
        patch?: never;
        trace?: never;
    };
    "/api/v1/player/resume": {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        get?: never;
        put?: never;
        post: operations["resume"];
        delete?: never;
        options?: never;
        head?: never;
        patch?: never;
        trace?: never;
    };
    "/api/v1/player/skip": {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        get?: never;
        put?: never;
        /** Fades out the track on air and starts the next one. */
        post: operations["skip"];
        delete?: never;
        options?: never;
        head?: never;
        patch?: never;
        trace?: never;
    };
    "/api/v1/playout/transition": {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        get?: never;
        put?: never;
        /**
         * Plans the transition between two tracks the way the engine would, for
         *     previews. Nothing is played.
         */
        post: operations["preview_transition"];
        delete?: never;
        options?: never;
        head?: never;
        patch?: never;
        trace?: never;
    };
    "/api/v1/queue": {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        /** The tracks waiting to play, in order: the cued one first. */
        get: operations["get_queue"];
        put?: never;
        /**
         * Queues library tracks. Missing files and tracks with nothing audible are
         *     refused.
         */
        post: operations["enqueue"];
        /**
         * Empties the queue, cued track included. The track on air plays on (and
         *     autofill, when on, queues new tracks).
         */
        delete: operations["clear_queue"];
        options?: never;
        head?: never;
        patch?: never;
        trace?: never;
    };
    "/api/v1/queue/{entry_id}": {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        get?: never;
        put?: never;
        post?: never;
        /** Takes an entry out of the queue. The track on air cannot be removed: skip it. */
        delete: operations["remove_entry"];
        options?: never;
        head?: never;
        patch?: never;
        trace?: never;
    };
    "/api/v1/queue/{entry_id}/position": {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        get?: never;
        /** Moves a waiting entry. */
        put: operations["move_entry"];
        post?: never;
        delete?: never;
        options?: never;
        head?: never;
        patch?: never;
        trace?: never;
    };
    "/api/v1/settings/analysis": {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        /** Gap Killer and crossfade point detection, used at import. */
        get: operations["get_analysis"];
        /**
         * Replaces the analysis settings. Every track is analyzed again with the
         *     new settings, in the background. Admins only.
         */
        put: operations["put_analysis"];
        post?: never;
        delete?: never;
        options?: never;
        head?: never;
        patch?: never;
        trace?: never;
    };
    "/api/v1/settings/library": {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        /** Identification and folder layout, used by `heron-librarian`. */
        get: operations["get_library"];
        put?: never;
        post?: never;
        delete?: never;
        options?: never;
        head?: never;
        /** Changes some library settings; the others are kept. Admins only. */
        patch: operations["patch_library"];
        trace?: never;
    };
    "/api/v1/settings/playout": {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        /** Crossfades and loudness normalization. */
        get: operations["get_playout"];
        /**
         * Replaces the playout settings (missing fields take the defaults) and
         *     pushes them to the engine. Admins only.
         */
        put: operations["put_playout"];
        post?: never;
        delete?: never;
        options?: never;
        head?: never;
        patch?: never;
        trace?: never;
    };
    "/api/v1/settings/queue": {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        get: operations["get_queue_settings"];
        /** Replaces the queue manager settings. Operators and admins. */
        put: operations["put_queue_settings"];
        post?: never;
        delete?: never;
        options?: never;
        head?: never;
        patch?: never;
        trace?: never;
    };
    "/api/v1/status": {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        /**
         * Whether the API, its database and the engine are up, and whether the
         *     server clock can be trusted. No sign-in needed.
         */
        get: operations["status"];
        put?: never;
        post?: never;
        delete?: never;
        options?: never;
        head?: never;
        patch?: never;
        trace?: never;
    };
    "/api/v1/tokens": {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        /** Your API tokens (not revoked). The tokens themselves are never shown again. */
        get: operations["list_tokens"];
        put?: never;
        /** Creates an API token acting as you, with your role. */
        post: operations["create_token"];
        delete?: never;
        options?: never;
        head?: never;
        patch?: never;
        trace?: never;
    };
    "/api/v1/tokens/{id}": {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        get?: never;
        put?: never;
        post?: never;
        /** Revokes one of your API tokens (admins: anyone's). */
        delete: operations["revoke_token"];
        options?: never;
        head?: never;
        patch?: never;
        trace?: never;
    };
    "/api/v1/tracks": {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        /** Library tracks, searched, filtered and sorted, a page at a time. */
        get: operations["list_tracks"];
        put?: never;
        post?: never;
        delete?: never;
        options?: never;
        head?: never;
        patch?: never;
        trace?: never;
    };
    "/api/v1/tracks/{id}": {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        /** A track with its analysis, overrides, categories and identification. */
        get: operations["get_track"];
        put?: never;
        post?: never;
        delete?: never;
        options?: never;
        head?: never;
        /** Enables or disables a track. Operators and admins. */
        patch: operations["patch_track"];
        trace?: never;
    };
    "/api/v1/tracks/{id}/audio": {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        /**
         * The track's audio file as it is in the library, for previews in the
         *     browser. Supports range requests.
         */
        get: operations["get_audio"];
        put?: never;
        post?: never;
        delete?: never;
        options?: never;
        head?: never;
        patch?: never;
        trace?: never;
    };
    "/api/v1/tracks/{id}/categories": {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        get?: never;
        /** Replaces the track's categories. Operators and admins. */
        put: operations["put_categories"];
        post?: never;
        delete?: never;
        options?: never;
        head?: never;
        patch?: never;
        trace?: never;
    };
    "/api/v1/tracks/{id}/overrides": {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        get?: never;
        /**
         * Replaces the track's overrides: cue points, Gap Killer, crossfade.
         *     Missing fields use the analysis and station settings. Queued entries of
         *     the track are updated within seconds. Operators and admins.
         */
        put: operations["put_overrides"];
        post?: never;
        /** Removes the track's overrides. Operators and admins. */
        delete: operations["delete_overrides"];
        options?: never;
        head?: never;
        patch?: never;
        trace?: never;
    };
    "/api/v1/tracks/{id}/review/apply": {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        get?: never;
        put?: never;
        /**
         * Applies one of the track's candidate releases: heron-librarian writes
         *     its tags into the file and files it where the layout says, as a batch
         *     that can be undone. Operators and admins.
         */
        post: operations["apply_release"];
        delete?: never;
        options?: never;
        head?: never;
        patch?: never;
        trace?: never;
    };
    "/api/v1/tracks/{id}/review/keep": {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        get?: never;
        put?: never;
        /**
         * Marks the track as reviewed: its file keeps its own tags. Operators and
         *     admins.
         */
        post: operations["keep_tags"];
        delete?: never;
        options?: never;
        head?: never;
        patch?: never;
        trace?: never;
    };
    "/api/v1/tracks/{id}/waveform": {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        /**
         * The track's waveform overview, computed with its analysis. 404 until
         *     the track is analyzed.
         */
        get: operations["get_waveform"];
        put?: never;
        post?: never;
        delete?: never;
        options?: never;
        head?: never;
        patch?: never;
        trace?: never;
    };
    "/api/v1/users": {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        /** Every user. Admins only. */
        get: operations["list_users"];
        put?: never;
        /** Admins only. */
        post: operations["create_user"];
        delete?: never;
        options?: never;
        head?: never;
        patch?: never;
        trace?: never;
    };
    "/api/v1/users/{id}": {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        get?: never;
        put?: never;
        post?: never;
        /** Deletes a user with their sessions and tokens. Admins only. */
        delete: operations["delete_user"];
        options?: never;
        head?: never;
        /**
         * Changes a user's role, enables or disables them, or resets their
         *     password. Admins only.
         */
        patch: operations["update_user"];
        trace?: never;
    };
}
export type webhooks = Record<string, never>;
export interface components {
    schemas: {
        AnalysisQueueStatus: {
            /** Format: int64 */
            failed: number;
            /** Format: int64 */
            pending: number;
            /** Format: int64 */
            running: number;
        };
        /**
         * @description Parameters of the import-time analysis (Gap Killer and crossfade point
         *     detection). Stored under the `analysis` settings name.
         *
         *     Levels are in dB **relative to the track's integrated loudness**, so the
         *     same thresholds work for quiet and loud masters. See `docs/analysis.md`.
         */
        AnalysisSettings: {
            /**
             * Format: int32
             * @description Bursts of sound this short, surrounded by silence, are ignored.
             * @default 10
             */
            blip_max_ms: number;
            /**
             * Format: double
             * @description Audio quieter than this at the end of a track is trimmed.
             * @default -24
             */
            end_threshold_db: number;
            /**
             * Format: int32
             * @description Longest distance between the crossfade point and the end of the track.
             * @default 2500
             */
            fade_max_ms: number;
            /**
             * Format: int32
             * @description Shortest distance between the crossfade point and the end of the track.
             * @default 150
             */
            fade_min_ms: number;
            /**
             * Format: double
             * @description The crossfade starts after the last moment the track is louder than this.
             * @default -15
             */
            fade_trigger_db: number;
            /**
             * Format: int32
             * @description Silence longer than this inside a track ends the track there.
             * @default 15000
             */
            max_internal_silence_ms: number;
            /**
             * Format: double
             * @description Absolute ceiling (dBFS) for what counts as silence: quiet but audible
             *     passages above it are never trimmed, whatever the track's loudness.
             * @default -50
             */
            silence_ceiling_dbfs: number;
            /**
             * Format: double
             * @description Audio quieter than this at the start of a track is trimmed.
             * @default -20
             */
            start_threshold_db: number;
        };
        ApiToken: {
            /** Format: int64 */
            created_at_unix_ms: number;
            /** Format: int64 */
            expires_at_unix_ms?: number | null;
            /** Format: int64 */
            id: number;
            /** Format: int64 */
            last_used_at_unix_ms?: number | null;
            name: string;
        };
        Batch: {
            /** Format: int64 */
            created_at_unix_ms: number;
            /**
             * Format: int64
             * @description Files moved or re-tagged.
             */
            files: number;
            /**
             * Format: int64
             * @description `None` while it runs, or when it was interrupted.
             */
            finished_at_unix_ms?: number | null;
            /** Format: int64 */
            id: number;
            kind: components["schemas"]["BatchKind"];
            /**
             * Format: int64
             * @description For an undo, the batch it undoes.
             */
            undoes?: number | null;
            /**
             * Format: int64
             * @description The undo of this batch.
             */
            undone_by?: number | null;
        };
        /** @enum {string} */
        BatchKind: "import" | "organize" | "undo" | "review";
        /** @description What a batch of file operations did. */
        BatchReport: {
            /** Format: int64 */
            batch_id: number;
            /** Format: int64 */
            done: number;
            /** @description Operations not done, with the reason. */
            skipped: components["schemas"]["SkippedFile"][];
        };
        Category: {
            /** @description `#rrggbb`. */
            color?: string | null;
            /** Format: int64 */
            id: number;
            name: string;
            /** Format: int64 */
            tracks: number;
        };
        CategoryAssignment: {
            /** Format: int64 */
            category_id: number;
            /**
             * Format: double
             * @description Relative chance of being picked within the category, 1 by default.
             */
            weight?: number;
        };
        CategoryInput: {
            /** @description `#rrggbb`, or none. */
            color?: string | null;
            name: string;
        };
        /** @description dBFS, −120 for silence. */
        ChannelLevels: {
            /** Format: float */
            peak_left: number;
            /** Format: float */
            peak_right: number;
            /** Format: float */
            rms_left: number;
            /** Format: float */
            rms_right: number;
        };
        ClockStatus: {
            /**
             * Format: double
             * @description The kernel's estimate of the clock's error, in ms.
             */
            estimated_error_ms?: number | null;
            /**
             * Format: double
             * @description The offset the kernel is correcting, in ms.
             */
            offset_ms?: number | null;
            /** @description `None` where the kernel's state cannot be read (not Linux). */
            synchronized?: boolean | null;
            /**
             * @description Why the clock cannot be trusted, for people. `None` when it is fine
             *     or unknown.
             */
            warning?: string | null;
        };
        CreatedToken: {
            /** Format: int64 */
            id: number;
            name: string;
            /** @description Send it as `Authorization: Bearer <token>`. Shown only once. */
            token: string;
        };
        /**
         * @description Where the next track starts.
         * @enum {string}
         */
        CrossfadeMode: "auto" | "fixed";
        /**
         * @description Per-track crossfade overrides (`track_overrides.crossfade`). Missing fields
         *     use the station settings. The fade-out, mode and fixed time apply to the
         *     transition out of the track; the fade-in to the transition into it.
         */
        CrossfadeOverrides: {
            /**
             * @description `false` plays the transition out of this track back to back, without overlap.
             * @default null
             */
            crossfade: boolean | null;
            /** @default null */
            fade_in: components["schemas"]["Fade"] | null;
            /** @default null */
            fade_out: components["schemas"]["Fade"] | null;
            /**
             * Format: int32
             * @default null
             */
            fixed_ms: number | null;
            /** @default null */
            mode: components["schemas"]["CrossfadeMode"] | null;
        };
        /**
         * @description Station-wide crossfade settings, modeled on SAM Broadcaster's
         *     "Cross-Fading" screen. Stored under `playout` in the `settings` table.
         */
        CrossfadeSettings: {
            /**
             * @description Fade-in of the incoming track.
             * @default {
             *       "curve": "linear",
             *       "enabled": false,
             *       "level_pct": 0,
             *       "time_ms": 1000
             *     }
             */
            fade_in: components["schemas"]["Fade"];
            /**
             * @description Fade-out of the outgoing track, from the moment the next one starts.
             * @default {
             *       "curve": "linear",
             *       "enabled": true,
             *       "level_pct": 0,
             *       "time_ms": 2500
             *     }
             */
            fade_out: components["schemas"]["Fade"];
            /**
             * Format: int32
             * @description With [`CrossfadeMode::Fixed`], how long before the cue-out the next track starts.
             * @default 2000
             */
            fixed_ms: number;
            /**
             * Format: int32
             * @description Tracks this long or shorter (jingles, IDs) are never crossfaded: they
             *     start right after the previous track and the next starts right after them.
             * @default 20000
             */
            min_track_ms: number;
            /** @default auto */
            mode: components["schemas"]["CrossfadeMode"];
            /**
             * Format: int32
             * @description Fade-out of the current track when it is skipped.
             * @default 1000
             */
            skip_fade_ms: number;
        };
        /** @enum {string} */
        DeckName: "a" | "b" | "fx";
        DeckPosition: {
            deck: components["schemas"]["DeckName"];
            /** Format: int64 */
            length_ms: number;
            /** Format: int64 */
            position_ms: number;
        };
        DeckState: {
            deck: components["schemas"]["DeckName"];
            entry?: components["schemas"]["QueueEntry"] | null;
            /** Format: int64 */
            length_ms: number;
            /** @description The track listeners hear as "now playing". */
            on_air: boolean;
            /**
             * Format: int64
             * @description From the cue-in.
             */
            position_ms: number;
        };
        /** @description How the engine will play the track: analysis and overrides combined. */
        EffectivePlayout: {
            /** Format: int32 */
            cue_in_ms: number;
            /** Format: int32 */
            cue_out_ms: number;
            /**
             * Format: double
             * @description Loudness normalization gain with the current playout settings.
             */
            gain_db: number;
            /** Format: int32 */
            mix_point_ms?: number | null;
        };
        /** @enum {string} */
        EncoderKind: "mp3" | "aac" | "opus" | "vorbis" | "external_ffmpeg";
        EncoderProfileInput: {
            /** Format: int32 */
            bitrate_kbps?: number | null;
            /** Format: int32 */
            channels: number;
            encoder: components["schemas"]["EncoderKind"];
            name: string;
            options?: Record<string, never>;
            /** Format: int32 */
            sample_rate: number;
        };
        EncoderProfileView: {
            /** Format: int32 */
            bitrate_kbps?: number | null;
            /** Format: int32 */
            channels: number;
            encoder: components["schemas"]["EncoderKind"];
            /** Format: int64 */
            id: number;
            name: string;
            /** @description Encoder-specific options, e.g. MP3: `{"mode": "cbr", "quality": 3}`. */
            options: Record<string, never>;
            /**
             * Format: int32
             * @description Outputs using this profile.
             */
            outputs: number;
            /** Format: int32 */
            sample_rate: number;
        };
        /** @enum {string} */
        EndReason: "finished" | "skipped" | "stopped" | "replaced";
        /** @description Whether the engine is reachable and runs the stored configuration. */
        EngineStatus: {
            /** @description Playout settings and outputs. */
            config: components["schemas"]["SyncStatus"];
            connected: boolean;
            /** Format: int64 */
            started_at_unix_ms?: number | null;
        };
        EnqueueRequest: {
            /**
             * Format: int32
             * @description Where among the waiting tracks, 0 being next. At the end when missing.
             */
            position?: number | null;
            /** @description Library tracks, played in this order. */
            track_ids: number[];
        };
        EnqueueResponse: {
            entry_ids: number[];
        };
        /** @description Body of every error response. */
        ErrorBody: {
            /**
             * @description Machine-readable code: `invalid_request`, `unauthorized`, `forbidden`,
             *     `not_found`, `conflict`, `invalid`, `too_many_requests`,
             *     `engine_unavailable` or `internal`.
             */
            error: string;
            /** @description For people. */
            message: string;
        };
        /** @description One side of a crossfade. */
        Fade: {
            /** @default linear */
            curve: components["schemas"]["FadeCurve"];
            /** @default true */
            enabled: boolean;
            /**
             * Format: int32
             * @description Level the fade-out goes down to, or the fade-in starts from, in percent.
             * @default 0
             */
            level_pct: number;
            /**
             * Format: int32
             * @default 2500
             */
            time_ms: number;
        };
        /**
         * @description Shape of a fade.
         * @enum {string}
         */
        FadeCurve: "linear" | "s_curve" | "logarithmic" | "exponential";
        Levels: {
            deck_a: components["schemas"]["ChannelLevels"];
            deck_b: components["schemas"]["ChannelLevels"];
            fx: components["schemas"]["ChannelLevels"];
            master: components["schemas"]["ChannelLevels"];
        };
        /** @description Changes to the library settings: missing fields are kept. */
        LibrarySettingsPatch: {
            /** @description The station's AcoustID application key; `null` removes it. */
            acoustid_key?: string | null;
            embed_cover?: boolean | null;
            /** @description See `docs/library.md#layout`. */
            layout?: string | null;
            /**
             * Format: double
             * @description Lowest AcoustID score (0 to 1) accepted as a match.
             */
            min_score?: number | null;
            save_cover?: boolean | null;
        };
        /** @description Library settings as shown: the AcoustID key is never sent back. */
        LibrarySettingsView: {
            /** @description Whether the station has set its AcoustID key (identification is off without one). */
            acoustid_key_set: boolean;
            embed_cover: boolean;
            layout: string;
            /** Format: double */
            min_score: number;
            save_cover: boolean;
        };
        /** @description Library and analysis progress, served by `/api/v1/library/status`. */
        LibraryStatus: {
            analysis_queue: components["schemas"]["AnalysisQueueStatus"];
            /**
             * Format: int64
             * @description Present tracks with an up-to-date analysis.
             */
            analyzed: number;
            last_scan?: components["schemas"]["ScanReport"] | null;
            /** Format: int64 */
            missing: number;
            /**
             * Format: int64
             * @description Analyzed tracks with nothing audible.
             */
            silent: number;
            /**
             * Format: int64
             * @description Tracks whose file is present.
             */
            tracks: number;
        };
        LibraryTask: {
            /** Format: int64 */
            created_at_unix_ms: number;
            /** @description Why it failed. */
            error?: string | null;
            /** Format: int64 */
            finished_at_unix_ms?: number | null;
            /** Format: int64 */
            id: number;
            plan?: components["schemas"]["OrganizePlan"] | null;
            report?: components["schemas"]["BatchReport"] | null;
            request: components["schemas"]["TaskRequest"];
            requested_by?: string | null;
            /** Format: int64 */
            started_at_unix_ms?: number | null;
            status: components["schemas"]["TaskStatus"];
        };
        /** @description A message on the live WebSocket. */
        LiveMessage: {
            player?: components["schemas"]["PlayerState"] | null;
            /** @enum {string} */
            type: "state";
        } | {
            /** Format: int64 */
            at_unix_ms: number;
            decks: components["schemas"]["DeckPosition"][];
            /** @enum {string} */
            type: "positions";
        } | {
            levels: components["schemas"]["Levels"];
            /** @enum {string} */
            type: "levels";
        } | {
            event: components["schemas"]["TrackEvent"];
            /** @enum {string} */
            type: "track";
        };
        LoginRequest: {
            password: string;
            username: string;
        };
        /** @description Loudness normalization from the analyzed integrated loudness (EBU R128). */
        LoudnessSettings: {
            /** @default true */
            enabled: boolean;
            /**
             * Format: double
             * @description Quiet tracks are boosted by at most this much.
             * @default 12
             */
            max_gain_db: number;
            /**
             * Format: double
             * @description Boosts never push the track's true peak above this. There is no
             *     limiter before M9, so a boost that clips would distort.
             * @default -1
             */
            peak_ceiling_dbtp: number;
            /**
             * Format: double
             * @default -16
             */
            target_lufs: number;
        };
        MoveRequest: {
            /**
             * Format: int32
             * @description New place among the waiting tracks, 0 being next.
             */
            position: number;
        };
        NewToken: {
            /**
             * Format: int32
             * @description Never expires when missing.
             */
            expires_in_days?: number | null;
            /** @description What the token is for. */
            name: string;
        };
        NewUser: {
            /** @description At least 8 characters. */
            password: string;
            role: components["schemas"]["Role"];
            username: string;
        };
        /** @description The dry run of an organization. */
        OrganizePlan: {
            moves: components["schemas"]["PlannedMove"][];
            /**
             * Format: int64
             * @description Tracks already where the layout puts them.
             */
            unchanged: number;
            /** @description Tracks whose file cannot be read (left where they are). */
            unreadable: string[];
        };
        /** @enum {string} */
        OutputConnection: "stopped" | "connecting" | "connected" | "reconnecting";
        OutputInput: {
            /** @description Sink settings. Credentials left out keep their stored value. */
            config: Record<string, never>;
            enabled?: boolean;
            /** Format: int64 */
            encoder_profile_id: number;
            kind: components["schemas"]["OutputKind"];
            metadata_enabled?: boolean;
            name: string;
        };
        /** @enum {string} */
        OutputKind: "icecast" | "shoutcast_v1" | "shoutcast_v2" | "hls" | "file" | "pipe";
        /** @description The live state of an output, as the engine reports it. */
        OutputStatus: {
            /** Format: int64 */
            bytes_sent: number;
            /** Format: int64 */
            connected_since_unix_ms?: number | null;
            /** Format: int64 */
            dropped_chunks: number;
            enabled: boolean;
            /** @description Why the last connection failed, while reconnecting. */
            error?: string | null;
            /** Format: int64 */
            id: number;
            name: string;
            state: components["schemas"]["OutputConnection"];
        };
        OutputView: {
            /** @description Sink settings (Icecast: host, port, mount…), without credentials. */
            config: Record<string, never>;
            enabled: boolean;
            /** Format: int64 */
            encoder_profile_id: number;
            /** Format: int64 */
            id: number;
            kind: components["schemas"]["OutputKind"];
            metadata_enabled: boolean;
            name: string;
            /** @description Credentials set in the settings (e.g. `password`), never sent back. */
            secrets_set: string[];
            status?: components["schemas"]["OutputStatus"] | null;
        };
        PasswordChange: {
            current_password: string;
            new_password: string;
        };
        PlannedMove: {
            /** @description Relative to the library root. */
            from: string;
            to: string;
            /** Format: int64 */
            track_id: number;
        };
        /** @description One play. */
        Play: {
            album?: string | null;
            artist?: string | null;
            deck: components["schemas"]["DeckName"];
            end_reason?: components["schemas"]["EndReason"] | null;
            /**
             * Format: int64
             * @description Missing while the track plays.
             */
            ended_at_unix_ms?: number | null;
            /** Format: int64 */
            id: number;
            /** Format: int32 */
            played_ms?: number | null;
            source: components["schemas"]["PlaySource"];
            /** Format: int64 */
            started_at_unix_ms: number;
            title?: string | null;
            /**
             * Format: int64
             * @description Missing for files outside the library, and tracks deleted since.
             */
            track_id?: number | null;
        };
        PlayFxRequest: {
            /** Format: int64 */
            track_id: number;
        };
        PlayFxResponse: {
            /** Format: int64 */
            entry_id: number;
        };
        /**
         * @description Why a track is played (`play_history.source`).
         * @enum {string}
         */
        PlaySource: "rotation" | "request" | "fallback" | "manual";
        /** @description Decks, queue and outputs of the engine. */
        PlayerState: {
            cued?: components["schemas"]["QueueEntry"] | null;
            decks: components["schemas"]["DeckState"][];
            /** Format: int64 */
            engine_started_at_unix_ms: number;
            outputs: components["schemas"]["OutputStatus"][];
            paused: boolean;
            /** @description After the cued track. */
            queue: components["schemas"]["QueueEntry"][];
            /**
             * Format: int64
             * @description Audio rendered since the engine started.
             */
            rendered_ms: number;
            /** Format: int64 */
            underruns: number;
        };
        /**
         * @description Playout settings, stored under `playout` in the `settings` table and
         *     pushed to the engine.
         */
        PlayoutSettings: {
            /**
             * @default {
             *       "fade_in": {
             *         "curve": "linear",
             *         "enabled": false,
             *         "level_pct": 0,
             *         "time_ms": 1000
             *       },
             *       "fade_out": {
             *         "curve": "linear",
             *         "enabled": true,
             *         "level_pct": 0,
             *         "time_ms": 2500
             *       },
             *       "fixed_ms": 2000,
             *       "min_track_ms": 20000,
             *       "mode": "auto",
             *       "skip_fade_ms": 1000
             *     }
             */
            crossfade: components["schemas"]["CrossfadeSettings"];
            /**
             * @default {
             *       "enabled": true,
             *       "max_gain_db": 12,
             *       "peak_ceiling_dbtp": -1,
             *       "target_lufs": -16
             *     }
             */
            loudness: components["schemas"]["LoudnessSettings"];
        };
        Preferences: {
            theme: components["schemas"]["Theme"];
        };
        QueueEntry: {
            /**
             * Format: int64
             * @description Assigned by the engine, unique until it restarts.
             */
            entry_id: number;
            track: components["schemas"]["QueuedTrack"];
        };
        /**
         * @description The queue manager. Until rotation and clockwheels (M6), it keeps the
         *     engine's queue filled with the least recently played tracks.
         */
        QueueSettings: {
            /**
             * @description Add tracks when the queue runs low. Off, the station plays silence
             *     once the queue is empty.
             * @default true
             */
            autofill: boolean;
            /**
             * Format: int32
             * @description Tracks kept waiting (cued and queued), 1 to 50.
             * @default 3
             */
            min_queued: number;
        };
        /** @description A track as the engine holds it. */
        QueuedTrack: {
            album?: string | null;
            artist?: string | null;
            /**
             * Format: int32
             * @description Cue points the engine plays between, overrides applied.
             */
            cue_in_ms: number;
            /** Format: int32 */
            cue_out_ms: number;
            /** Format: int32 */
            duration_ms: number;
            /** @description Relative to the library root. */
            path: string;
            source: components["schemas"]["PlaySource"];
            title?: string | null;
            /**
             * Format: int64
             * @description Library track, when the track comes from the library.
             */
            track_id?: number | null;
        };
        /** @description A gain ramp, in ms on its track's own timeline. */
        RampView: {
            curve: components["schemas"]["FadeCurve"];
            /** Format: int32 */
            length_ms: number;
            /**
             * Format: float
             * @description Level at the quiet end of the ramp, 0 to 1.
             */
            level: number;
            /** Format: int32 */
            start_ms: number;
        };
        ReleaseChoice: {
            recording_id: string;
            release_id: string;
        };
        /** @description A release a track may be on, from AcoustID. */
        ReviewCandidate: {
            country?: string | null;
            /** @description `YYYY-MM-DD`, `YYYY-MM` or `YYYY`. */
            date?: string | null;
            /** Format: int32 */
            disc?: number | null;
            recording_id: string;
            release_group_id?: string | null;
            release_group_title?: string | null;
            release_id: string;
            release_title?: string | null;
            /** Format: double */
            score: number;
            /** Format: int32 */
            track?: number | null;
        };
        ReviewItem: {
            /** @description Best first. */
            candidates: components["schemas"]["ReviewCandidate"][];
            /** @description Why the identification was not applied. */
            reason?: string | null;
            /** Format: double */
            score?: number | null;
            track: components["schemas"]["TrackSummary"];
        };
        ReviewPage: {
            items: components["schemas"]["ReviewItem"][];
            /** Format: int64 */
            total: number;
        };
        /**
         * @description What a user may do. Each role can do everything the previous one can.
         * @enum {string}
         */
        Role: "viewer" | "operator" | "admin";
        /** @description Outcome of one scan, logged and served by `/api/v1/library/status`. */
        ScanReport: {
            /** Format: int64 */
            added: number;
            /**
             * Format: int64
             * @description Files modified too recently (probably still being copied).
             */
            deferred: number;
            /** Format: int64 */
            duration_ms: number;
            /**
             * Format: int64
             * @description Analysis jobs queued.
             */
            enqueued: number;
            /**
             * Format: int64
             * @description Files that could not be read.
             */
            failed: number;
            /**
             * Format: int64
             * @description Audio files found on disk.
             */
            files: number;
            /** Format: int64 */
            finished_at_unix: number;
            /**
             * Format: int64
             * @description Tracks newly marked missing.
             */
            missing: number;
            /**
             * Format: int64
             * @description Files found under a new path (same content as a missing track).
             */
            moved: number;
            /**
             * Format: int64
             * @description Missing tracks whose file is back.
             */
            restored: number;
            /** Format: int64 */
            unchanged: number;
            /** Format: int64 */
            updated: number;
        };
        SkippedFile: {
            path: string;
            reason: string;
        };
        Status: {
            /** @description The server clock's synchronization (the host's: containers share it). */
            clock: components["schemas"]["ClockStatus"];
            /** @description `ok` or `unavailable`. */
            database: string;
            /** @description `ok` or `unavailable`. */
            engine: string;
            /**
             * Format: int64
             * @description The server's time, to compare with the browser's.
             */
            server_time_unix_ms: number;
            service: string;
            version: string;
        };
        /** @enum {string} */
        SyncState: "unknown" | "synced" | "pending" | "failed";
        /** @description Whether the engine runs with the configuration stored in the database. */
        SyncStatus: {
            /** Format: int64 */
            checked_at_unix_ms?: number | null;
            error?: string | null;
            state: components["schemas"]["SyncState"];
        };
        /** @description A task, as stored in `library_tasks.params`. The tag is also its `kind`. */
        TaskRequest: {
            /** @enum {string} */
            kind: "organize_plan";
        } | {
            /** @enum {string} */
            kind: "organize_apply";
            /** Format: int64 */
            plan_task_id: number;
        } | {
            /** Format: int64 */
            batch_id: number;
            /** @enum {string} */
            kind: "undo";
        } | {
            /** Format: int32 */
            disc?: number | null;
            /** @enum {string} */
            kind: "review_apply";
            recording_id: string;
            release_id: string;
            /** Format: int32 */
            track?: number | null;
            /** Format: int64 */
            track_id: number;
        };
        /** @enum {string} */
        TaskStatus: "pending" | "running" | "done" | "failed";
        /**
         * @description The web UI's theme.
         * @enum {string}
         */
        Theme: "system" | "light" | "dark";
        TrackAnalysis: {
            /** Format: int64 */
            analyzed_at_unix_ms: number;
            /** Format: int32 */
            cue_in_ms: number;
            /** Format: int32 */
            cue_out_ms: number;
            /**
             * Format: int32
             * @description The crossfade point: where the next track starts.
             */
            fade_out_start_ms?: number | null;
            has_internal_silence: boolean;
            /** Format: double */
            integrated_lufs?: number | null;
            /** Format: double */
            true_peak_db?: number | null;
        };
        TrackCategory: {
            /** Format: int64 */
            category_id: number;
            color?: string | null;
            name: string;
            /** Format: double */
            weight: number;
        };
        TrackDetail: components["schemas"]["TrackSummary"] & {
            /** Format: int64 */
            added_at_unix_ms: number;
            analysis?: components["schemas"]["TrackAnalysis"] | null;
            categories: components["schemas"]["TrackCategory"][];
            /** Format: int64 */
            file_size?: number | null;
            identification?: components["schemas"]["TrackIdentification"] | null;
            /** Format: int64 */
            missing_since_unix_ms?: number | null;
            overrides: components["schemas"]["TrackOverrides"];
            playout: components["schemas"]["EffectivePlayout"];
        };
        /** @description A track change reported by the engine. */
        TrackEvent: {
            /** Format: int64 */
            at_unix_ms: number;
            deck: components["schemas"]["DeckName"];
            entry: components["schemas"]["QueueEntry"];
            /** @enum {string} */
            kind: "started";
        } | {
            /** Format: int64 */
            at_unix_ms: number;
            deck: components["schemas"]["DeckName"];
            entry: components["schemas"]["QueueEntry"];
            /** @enum {string} */
            kind: "ended";
            /** Format: int64 */
            played_ms: number;
            reason: components["schemas"]["EndReason"];
        };
        TrackIdentification: {
            /** @description Why the track needs review. */
            reason?: string | null;
            recording_mbid?: string | null;
            release_mbid?: string | null;
            /** Format: double */
            score?: number | null;
            /** @description `identified` or `needs_review`. */
            status: string;
        };
        /**
         * @description Per-track overrides (`track_overrides`). Missing fields use the analysis
         *     and station settings.
         */
        TrackOverrides: {
            /**
             * @default {
             *       "crossfade": null,
             *       "fade_in": null,
             *       "fade_out": null,
             *       "fixed_ms": null,
             *       "mode": null
             *     }
             */
            crossfade: components["schemas"]["CrossfadeOverrides"];
            /**
             * Format: int32
             * @default null
             */
            cue_in_ms: number | null;
            /**
             * Format: int32
             * @default null
             */
            cue_out_ms: number | null;
            /** @default null */
            gap_killer_enabled: boolean | null;
        };
        TrackPage: {
            items: components["schemas"]["TrackSummary"][];
            /**
             * Format: int64
             * @description Matching tracks in all pages.
             */
            total: number;
        };
        TrackPatch: {
            enabled?: boolean | null;
        };
        TrackSummary: {
            album?: string | null;
            analyzed: boolean;
            artist?: string | null;
            /** Format: int32 */
            duration_ms?: number | null;
            /** @description Disabled tracks are never picked by autofill or rotation. */
            enabled: boolean;
            /** Format: int64 */
            id: number;
            /** Format: int64 */
            last_played_at_unix_ms?: number | null;
            missing: boolean;
            /** @description Relative to the library root. */
            path: string;
            silent: boolean;
            title?: string | null;
        };
        TransitionPreview: {
            fade_in?: components["schemas"]["RampView"] | null;
            fade_out?: components["schemas"]["RampView"] | null;
            incoming: components["schemas"]["EffectivePlayout"];
            /**
             * Format: int32
             * @description When the incoming track starts, on the outgoing track's timeline.
             */
            next_start_ms: number;
            outgoing: components["schemas"]["EffectivePlayout"];
            /**
             * Format: int32
             * @description How long both tracks play at once (0: back to back).
             */
            overlap_ms: number;
        };
        TransitionRequest: {
            /** Format: int64 */
            from_track_id: number;
            settings?: components["schemas"]["PlayoutSettings"] | null;
            /** Format: int64 */
            to_track_id: number;
        };
        User: {
            /** Format: int64 */
            created_at_unix_ms: number;
            enabled: boolean;
            /** Format: int64 */
            id: number;
            /** Format: int64 */
            last_login_at_unix_ms?: number | null;
            role: components["schemas"]["Role"];
            theme: components["schemas"]["Theme"];
            username: string;
        };
        UserPatch: {
            /** @description Disabled users cannot sign in, and their tokens stop working. */
            enabled?: boolean | null;
            /** @description Signs the user out everywhere. */
            password?: string | null;
            role?: components["schemas"]["Role"] | null;
        };
        Waveform: {
            /**
             * Format: float
             * @description Level of point 0; 255 is full scale (0 dBFS), on a dB scale.
             */
            floor_db: number;
            /**
             * Format: int32
             * @description Length of one point.
             */
            interval_ms: number;
            /** @description One byte per point, base64: the highest sample peak in the interval. */
            peaks: string;
        };
    };
    responses: never;
    parameters: never;
    requestBodies: never;
    headers: never;
    pathItems: never;
}
export type $defs = Record<string, never>;
export interface operations {
    login: {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        requestBody: {
            content: {
                "application/json": components["schemas"]["LoginRequest"];
            };
        };
        responses: {
            200: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["User"];
                };
            };
            401: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorBody"];
                };
            };
            /** @description Too many failures for this username */
            429: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorBody"];
                };
            };
        };
    };
    logout: {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        requestBody?: never;
        responses: {
            204: {
                headers: {
                    [name: string]: unknown;
                };
                content?: never;
            };
            "4XX": {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorBody"];
                };
            };
        };
    };
    me: {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        requestBody?: never;
        responses: {
            200: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["User"];
                };
            };
            "4XX": {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorBody"];
                };
            };
        };
    };
    change_password: {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        requestBody: {
            content: {
                "application/json": components["schemas"]["PasswordChange"];
            };
        };
        responses: {
            204: {
                headers: {
                    [name: string]: unknown;
                };
                content?: never;
            };
            "4XX": {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorBody"];
                };
            };
        };
    };
    set_preferences: {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        requestBody: {
            content: {
                "application/json": components["schemas"]["Preferences"];
            };
        };
        responses: {
            200: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["User"];
                };
            };
            "4XX": {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorBody"];
                };
            };
        };
    };
    list_categories: {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        requestBody?: never;
        responses: {
            200: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["Category"][];
                };
            };
            "4XX": {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorBody"];
                };
            };
        };
    };
    create_category: {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        requestBody: {
            content: {
                "application/json": components["schemas"]["CategoryInput"];
            };
        };
        responses: {
            201: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["Category"];
                };
            };
            "4XX": {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorBody"];
                };
            };
        };
    };
    get_category: {
        parameters: {
            query?: never;
            header?: never;
            path: {
                id: number;
            };
            cookie?: never;
        };
        requestBody?: never;
        responses: {
            200: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["Category"];
                };
            };
            "4XX": {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorBody"];
                };
            };
        };
    };
    update_category: {
        parameters: {
            query?: never;
            header?: never;
            path: {
                id: number;
            };
            cookie?: never;
        };
        requestBody: {
            content: {
                "application/json": components["schemas"]["CategoryInput"];
            };
        };
        responses: {
            200: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["Category"];
                };
            };
            "4XX": {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorBody"];
                };
            };
        };
    };
    delete_category: {
        parameters: {
            query?: never;
            header?: never;
            path: {
                id: number;
            };
            cookie?: never;
        };
        requestBody?: never;
        responses: {
            204: {
                headers: {
                    [name: string]: unknown;
                };
                content?: never;
            };
            "4XX": {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorBody"];
                };
            };
        };
    };
    list_profiles: {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        requestBody?: never;
        responses: {
            200: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["EncoderProfileView"][];
                };
            };
            "4XX": {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorBody"];
                };
            };
        };
    };
    create_profile: {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        requestBody: {
            content: {
                "application/json": components["schemas"]["EncoderProfileInput"];
            };
        };
        responses: {
            201: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["EncoderProfileView"];
                };
            };
            "4XX": {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorBody"];
                };
            };
        };
    };
    get_profile: {
        parameters: {
            query?: never;
            header?: never;
            path: {
                id: number;
            };
            cookie?: never;
        };
        requestBody?: never;
        responses: {
            200: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["EncoderProfileView"];
                };
            };
            "4XX": {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorBody"];
                };
            };
        };
    };
    update_profile: {
        parameters: {
            query?: never;
            header?: never;
            path: {
                id: number;
            };
            cookie?: never;
        };
        requestBody: {
            content: {
                "application/json": components["schemas"]["EncoderProfileInput"];
            };
        };
        responses: {
            200: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["EncoderProfileView"];
                };
            };
            "4XX": {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorBody"];
                };
            };
        };
    };
    delete_profile: {
        parameters: {
            query?: never;
            header?: never;
            path: {
                id: number;
            };
            cookie?: never;
        };
        requestBody?: never;
        responses: {
            204: {
                headers: {
                    [name: string]: unknown;
                };
                content?: never;
            };
            "4XX": {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorBody"];
                };
            };
        };
    };
    engine_status: {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        requestBody?: never;
        responses: {
            200: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["EngineStatus"];
                };
            };
            "4XX": {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorBody"];
                };
            };
        };
    };
    list_history: {
        parameters: {
            query?: {
                /** @description Plays started before this time (Unix ms), for the next page. */
                before?: number;
                /** @description Plays started at or after this time (Unix ms). */
                after?: number;
                /** @description Only this track. */
                track_id?: number;
                /** @description Leave out the FX player (jingles played over the music). */
                exclude_fx?: boolean;
                /** @description 1 to 500, 50 by default. */
                limit?: number;
            };
            header?: never;
            path?: never;
            cookie?: never;
        };
        requestBody?: never;
        responses: {
            200: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["Play"][];
                };
            };
            "4XX": {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorBody"];
                };
            };
        };
    };
    list_batches: {
        parameters: {
            query?: {
                /** @description 1 to 200, 20 by default. */
                limit?: number;
            };
            header?: never;
            path?: never;
            cookie?: never;
        };
        requestBody?: never;
        responses: {
            200: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["Batch"][];
                };
            };
            "4XX": {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorBody"];
                };
            };
        };
    };
    undo_batch: {
        parameters: {
            query?: never;
            header?: never;
            path: {
                id: number;
            };
            cookie?: never;
        };
        requestBody?: never;
        responses: {
            202: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["LibraryTask"];
                };
            };
            "4XX": {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorBody"];
                };
            };
        };
    };
    plan_organization: {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        requestBody?: never;
        responses: {
            202: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["LibraryTask"];
                };
            };
            "4XX": {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorBody"];
                };
            };
        };
    };
    apply_organization: {
        parameters: {
            query?: never;
            header?: never;
            path: {
                plan_task_id: number;
            };
            cookie?: never;
        };
        requestBody?: never;
        responses: {
            202: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["LibraryTask"];
                };
            };
            "4XX": {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorBody"];
                };
            };
        };
    };
    list_review: {
        parameters: {
            query?: {
                /** @description 1 to 200, 50 by default. */
                limit?: number;
                offset?: number;
            };
            header?: never;
            path?: never;
            cookie?: never;
        };
        requestBody?: never;
        responses: {
            200: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ReviewPage"];
                };
            };
            "4XX": {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorBody"];
                };
            };
        };
    };
    library_status: {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        requestBody?: never;
        responses: {
            200: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["LibraryStatus"];
                };
            };
            "5XX": {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorBody"];
                };
            };
        };
    };
    list_tasks: {
        parameters: {
            query?: {
                /** @description 1 to 200, 20 by default. */
                limit?: number;
            };
            header?: never;
            path?: never;
            cookie?: never;
        };
        requestBody?: never;
        responses: {
            200: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["LibraryTask"][];
                };
            };
            "4XX": {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorBody"];
                };
            };
        };
    };
    get_task: {
        parameters: {
            query?: never;
            header?: never;
            path: {
                id: number;
            };
            cookie?: never;
        };
        requestBody?: never;
        responses: {
            200: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["LibraryTask"];
                };
            };
            "4XX": {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorBody"];
                };
            };
        };
    };
    live: {
        parameters: {
            query?: {
                /** @description Send levels ten times a second (default true). */
                levels?: boolean;
            };
            header?: never;
            path?: never;
            cookie?: never;
        };
        requestBody?: never;
        responses: {
            /** @description WebSocket of LiveMessage */
            101: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["LiveMessage"];
                };
            };
            "4XX": {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorBody"];
                };
            };
        };
    };
    list_outputs: {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        requestBody?: never;
        responses: {
            200: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["OutputView"][];
                };
            };
            "4XX": {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorBody"];
                };
            };
        };
    };
    create_output: {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        requestBody: {
            content: {
                "application/json": components["schemas"]["OutputInput"];
            };
        };
        responses: {
            201: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["OutputView"];
                };
            };
            "4XX": {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorBody"];
                };
            };
        };
    };
    get_output: {
        parameters: {
            query?: never;
            header?: never;
            path: {
                id: number;
            };
            cookie?: never;
        };
        requestBody?: never;
        responses: {
            200: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["OutputView"];
                };
            };
            "4XX": {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorBody"];
                };
            };
        };
    };
    update_output: {
        parameters: {
            query?: never;
            header?: never;
            path: {
                id: number;
            };
            cookie?: never;
        };
        requestBody: {
            content: {
                "application/json": components["schemas"]["OutputInput"];
            };
        };
        responses: {
            200: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["OutputView"];
                };
            };
            "4XX": {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorBody"];
                };
            };
        };
    };
    delete_output: {
        parameters: {
            query?: never;
            header?: never;
            path: {
                id: number;
            };
            cookie?: never;
        };
        requestBody?: never;
        responses: {
            204: {
                headers: {
                    [name: string]: unknown;
                };
                content?: never;
            };
            "4XX": {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorBody"];
                };
            };
        };
    };
    get_player: {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        requestBody?: never;
        responses: {
            200: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["PlayerState"];
                };
            };
            /** @description Engine unreachable */
            503: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorBody"];
                };
            };
            "4XX": {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorBody"];
                };
            };
        };
    };
    play_fx: {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        requestBody: {
            content: {
                "application/json": components["schemas"]["PlayFxRequest"];
            };
        };
        responses: {
            200: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["PlayFxResponse"];
                };
            };
            503: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorBody"];
                };
            };
            "4XX": {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorBody"];
                };
            };
        };
    };
    stop_fx: {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        requestBody?: never;
        responses: {
            204: {
                headers: {
                    [name: string]: unknown;
                };
                content?: never;
            };
            503: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorBody"];
                };
            };
            "4XX": {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorBody"];
                };
            };
        };
    };
    get_levels: {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        requestBody?: never;
        responses: {
            200: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["Levels"];
                };
            };
            /** @description Engine unreachable */
            503: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorBody"];
                };
            };
            "4XX": {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorBody"];
                };
            };
        };
    };
    pause: {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        requestBody?: never;
        responses: {
            204: {
                headers: {
                    [name: string]: unknown;
                };
                content?: never;
            };
            503: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorBody"];
                };
            };
            "4XX": {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorBody"];
                };
            };
        };
    };
    resume: {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        requestBody?: never;
        responses: {
            204: {
                headers: {
                    [name: string]: unknown;
                };
                content?: never;
            };
            503: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorBody"];
                };
            };
            "4XX": {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorBody"];
                };
            };
        };
    };
    skip: {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        requestBody?: never;
        responses: {
            204: {
                headers: {
                    [name: string]: unknown;
                };
                content?: never;
            };
            503: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorBody"];
                };
            };
            "4XX": {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorBody"];
                };
            };
        };
    };
    preview_transition: {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        requestBody: {
            content: {
                "application/json": components["schemas"]["TransitionRequest"];
            };
        };
        responses: {
            200: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["TransitionPreview"];
                };
            };
            "4XX": {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorBody"];
                };
            };
        };
    };
    get_queue: {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        requestBody?: never;
        responses: {
            200: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["QueueEntry"][];
                };
            };
            503: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorBody"];
                };
            };
            "4XX": {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorBody"];
                };
            };
        };
    };
    enqueue: {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        requestBody: {
            content: {
                "application/json": components["schemas"]["EnqueueRequest"];
            };
        };
        responses: {
            200: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["EnqueueResponse"];
                };
            };
            503: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorBody"];
                };
            };
            "4XX": {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorBody"];
                };
            };
        };
    };
    clear_queue: {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        requestBody?: never;
        responses: {
            204: {
                headers: {
                    [name: string]: unknown;
                };
                content?: never;
            };
            503: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorBody"];
                };
            };
            "4XX": {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorBody"];
                };
            };
        };
    };
    remove_entry: {
        parameters: {
            query?: never;
            header?: never;
            path: {
                entry_id: number;
            };
            cookie?: never;
        };
        requestBody?: never;
        responses: {
            204: {
                headers: {
                    [name: string]: unknown;
                };
                content?: never;
            };
            503: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorBody"];
                };
            };
            "4XX": {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorBody"];
                };
            };
        };
    };
    move_entry: {
        parameters: {
            query?: never;
            header?: never;
            path: {
                entry_id: number;
            };
            cookie?: never;
        };
        requestBody: {
            content: {
                "application/json": components["schemas"]["MoveRequest"];
            };
        };
        responses: {
            204: {
                headers: {
                    [name: string]: unknown;
                };
                content?: never;
            };
            503: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorBody"];
                };
            };
            "4XX": {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorBody"];
                };
            };
        };
    };
    get_analysis: {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        requestBody?: never;
        responses: {
            200: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["AnalysisSettings"];
                };
            };
            "4XX": {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorBody"];
                };
            };
        };
    };
    put_analysis: {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        requestBody: {
            content: {
                "application/json": components["schemas"]["AnalysisSettings"];
            };
        };
        responses: {
            200: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["AnalysisSettings"];
                };
            };
            "4XX": {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorBody"];
                };
            };
        };
    };
    get_library: {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        requestBody?: never;
        responses: {
            200: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["LibrarySettingsView"];
                };
            };
            "4XX": {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorBody"];
                };
            };
        };
    };
    patch_library: {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        requestBody: {
            content: {
                "application/json": components["schemas"]["LibrarySettingsPatch"];
            };
        };
        responses: {
            200: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["LibrarySettingsView"];
                };
            };
            "4XX": {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorBody"];
                };
            };
        };
    };
    get_playout: {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        requestBody?: never;
        responses: {
            200: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["PlayoutSettings"];
                };
            };
            "4XX": {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorBody"];
                };
            };
        };
    };
    put_playout: {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        requestBody: {
            content: {
                "application/json": components["schemas"]["PlayoutSettings"];
            };
        };
        responses: {
            200: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["PlayoutSettings"];
                };
            };
            "4XX": {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorBody"];
                };
            };
        };
    };
    get_queue_settings: {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        requestBody?: never;
        responses: {
            200: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["QueueSettings"];
                };
            };
            "4XX": {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorBody"];
                };
            };
        };
    };
    put_queue_settings: {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        requestBody: {
            content: {
                "application/json": components["schemas"]["QueueSettings"];
            };
        };
        responses: {
            200: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["QueueSettings"];
                };
            };
            "4XX": {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorBody"];
                };
            };
        };
    };
    status: {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        requestBody?: never;
        responses: {
            200: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["Status"];
                };
            };
        };
    };
    list_tokens: {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        requestBody?: never;
        responses: {
            200: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ApiToken"][];
                };
            };
            "4XX": {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorBody"];
                };
            };
        };
    };
    create_token: {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        requestBody: {
            content: {
                "application/json": components["schemas"]["NewToken"];
            };
        };
        responses: {
            201: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["CreatedToken"];
                };
            };
            "4XX": {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorBody"];
                };
            };
        };
    };
    revoke_token: {
        parameters: {
            query?: never;
            header?: never;
            path: {
                id: number;
            };
            cookie?: never;
        };
        requestBody?: never;
        responses: {
            204: {
                headers: {
                    [name: string]: unknown;
                };
                content?: never;
            };
            "4XX": {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorBody"];
                };
            };
        };
    };
    list_tracks: {
        parameters: {
            query?: {
                /** @description Words found in the title, artist, album or path. */
                q?: string;
                category_id?: number;
                filter?: "playable" | "missing" | "unanalyzed" | "silent" | "disabled" | "needs_review";
                sort?: "artist" | "title" | "album" | "added" | "last_played";
                /** @description 1 to 500, 50 by default. */
                limit?: number;
                offset?: number;
            };
            header?: never;
            path?: never;
            cookie?: never;
        };
        requestBody?: never;
        responses: {
            200: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["TrackPage"];
                };
            };
            "4XX": {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorBody"];
                };
            };
        };
    };
    get_track: {
        parameters: {
            query?: never;
            header?: never;
            path: {
                id: number;
            };
            cookie?: never;
        };
        requestBody?: never;
        responses: {
            200: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["TrackDetail"];
                };
            };
            "4XX": {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorBody"];
                };
            };
        };
    };
    patch_track: {
        parameters: {
            query?: never;
            header?: never;
            path: {
                id: number;
            };
            cookie?: never;
        };
        requestBody: {
            content: {
                "application/json": components["schemas"]["TrackPatch"];
            };
        };
        responses: {
            200: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["TrackDetail"];
                };
            };
            "4XX": {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorBody"];
                };
            };
        };
    };
    get_audio: {
        parameters: {
            query?: never;
            header?: never;
            path: {
                id: number;
            };
            cookie?: never;
        };
        requestBody?: never;
        responses: {
            /** @description The file */
            200: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/octet-stream": unknown;
                };
            };
            /** @description Part of the file */
            206: {
                headers: {
                    [name: string]: unknown;
                };
                content?: never;
            };
            "4XX": {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorBody"];
                };
            };
        };
    };
    put_categories: {
        parameters: {
            query?: never;
            header?: never;
            path: {
                id: number;
            };
            cookie?: never;
        };
        requestBody: {
            content: {
                "application/json": components["schemas"]["CategoryAssignment"][];
            };
        };
        responses: {
            200: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["TrackCategory"][];
                };
            };
            "4XX": {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorBody"];
                };
            };
        };
    };
    put_overrides: {
        parameters: {
            query?: never;
            header?: never;
            path: {
                id: number;
            };
            cookie?: never;
        };
        requestBody: {
            content: {
                "application/json": components["schemas"]["TrackOverrides"];
            };
        };
        responses: {
            200: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["TrackOverrides"];
                };
            };
            "4XX": {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorBody"];
                };
            };
        };
    };
    delete_overrides: {
        parameters: {
            query?: never;
            header?: never;
            path: {
                id: number;
            };
            cookie?: never;
        };
        requestBody?: never;
        responses: {
            204: {
                headers: {
                    [name: string]: unknown;
                };
                content?: never;
            };
            "4XX": {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorBody"];
                };
            };
        };
    };
    apply_release: {
        parameters: {
            query?: never;
            header?: never;
            path: {
                id: number;
            };
            cookie?: never;
        };
        requestBody: {
            content: {
                "application/json": components["schemas"]["ReleaseChoice"];
            };
        };
        responses: {
            202: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["LibraryTask"];
                };
            };
            "4XX": {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorBody"];
                };
            };
        };
    };
    keep_tags: {
        parameters: {
            query?: never;
            header?: never;
            path: {
                id: number;
            };
            cookie?: never;
        };
        requestBody?: never;
        responses: {
            204: {
                headers: {
                    [name: string]: unknown;
                };
                content?: never;
            };
            "4XX": {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorBody"];
                };
            };
        };
    };
    get_waveform: {
        parameters: {
            query?: never;
            header?: never;
            path: {
                id: number;
            };
            cookie?: never;
        };
        requestBody?: never;
        responses: {
            200: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["Waveform"];
                };
            };
            "4XX": {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorBody"];
                };
            };
        };
    };
    list_users: {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        requestBody?: never;
        responses: {
            200: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["User"][];
                };
            };
            "4XX": {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorBody"];
                };
            };
        };
    };
    create_user: {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        requestBody: {
            content: {
                "application/json": components["schemas"]["NewUser"];
            };
        };
        responses: {
            201: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["User"];
                };
            };
            "4XX": {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorBody"];
                };
            };
        };
    };
    delete_user: {
        parameters: {
            query?: never;
            header?: never;
            path: {
                id: number;
            };
            cookie?: never;
        };
        requestBody?: never;
        responses: {
            204: {
                headers: {
                    [name: string]: unknown;
                };
                content?: never;
            };
            "4XX": {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorBody"];
                };
            };
        };
    };
    update_user: {
        parameters: {
            query?: never;
            header?: never;
            path: {
                id: number;
            };
            cookie?: never;
        };
        requestBody: {
            content: {
                "application/json": components["schemas"]["UserPatch"];
            };
        };
        responses: {
            200: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["User"];
                };
            };
            "4XX": {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorBody"];
                };
            };
        };
    };
}
