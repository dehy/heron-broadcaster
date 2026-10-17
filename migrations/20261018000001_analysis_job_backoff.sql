-- SPDX-License-Identifier: AGPL-3.0-or-later
--
-- Retries and postponed jobs wait until not_before, instead of being
-- claimed again at once (files still being copied, transient errors).
ALTER TABLE analysis_jobs
    ADD COLUMN not_before DATETIME(6) NULL AFTER status,
    DROP KEY analysis_jobs_status,
    ADD KEY analysis_jobs_status (status, not_before);
