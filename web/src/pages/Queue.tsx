// SPDX-License-Identifier: AGPL-3.0-or-later

// The queue: reorder by dragging, add tracks from the library, autofill.

import { useQuery } from "@tanstack/react-query";
import { api, done, unwrap } from "../api";
import { QueueList, useWaiting } from "../components/QueueList";
import { TrackSearch } from "../components/TrackSearch";
import { Card, ConfirmButton, Icon, PageHead } from "../components/ui";
import { count, duration } from "../lib/format";
import { useAction, useCan } from "../lib/session";

export function QueuePage() {
  const waiting = useWaiting();
  const canEdit = useCan("operator");
  const settings = useQuery({
    queryKey: ["settings", "queue"],
    queryFn: () => unwrap(api.GET("/api/v1/settings/queue")),
  });

  const enqueue = useAction(
    ({ id, next }: { id: number; next: boolean }) =>
      unwrap(
        api.POST("/api/v1/queue", {
          body: next ? { track_ids: [id], position: 0 } : { track_ids: [id] },
        }),
      ),
    { success: "Queued" },
  );
  const clear = useAction(() => done(api.DELETE("/api/v1/queue")), { success: "Queue cleared" });
  const setAutofill = useAction(
    (autofill: boolean) =>
      unwrap(
        api.PUT("/api/v1/settings/queue", {
          body: { autofill, min_queued: settings.data?.min_queued ?? 3 },
        }),
      ),
    { invalidate: [["settings", "queue"]] },
  );

  const total =
    waiting?.reduce((sum, entry) => sum + Math.max(0, entry.track.cue_out_ms - entry.track.cue_in_ms), 0) ??
    0;

  return (
    <>
      <PageHead
        title="Queue"
        description={
          waiting
            ? `${count(waiting.length, "track")} waiting, ${duration(total)} of music. Drag tracks to reorder them.`
            : undefined
        }
        actions={
          canEdit && (
            <>
              {settings.data && (
                <label className="check" title="Queue tracks when fewer than the minimum wait">
                  <input
                    type="checkbox"
                    checked={settings.data.autofill}
                    disabled={setAutofill.isPending}
                    onChange={(event) => setAutofill.mutate(event.target.checked)}
                  />
                  Autofill (at least {settings.data.min_queued})
                </label>
              )}
              <ConfirmButton
                confirm="Clear the queue"
                question={
                  <p>
                    Every waiting track leaves the queue, the cued one too. The track on air plays
                    on{settings.data?.autofill ? ", and autofill queues new tracks" : ""}.
                  </p>
                }
                onConfirm={() => clear.mutate(undefined)}
                disabled={!waiting?.length}
              >
                <Icon name="trash" /> Clear
              </ConfirmButton>
            </>
          )
        }
      />
      {settings.data && !settings.data.autofill && (
        <div className="banner warn" role="status">
          <Icon name="warn" /> Autofill is off: the station goes silent once the queue is empty.
        </div>
      )}
      <div className="grid sidebar-right">
        <Card flush>
          <QueueList />
        </Card>
        {canEdit && (
          <Card title="Add tracks">
            <TrackSearch
              actions={(track) => (
                <>
                  <button
                    type="button"
                    className="btn small"
                    title="Play next"
                    onClick={() => enqueue.mutate({ id: track.id, next: true })}
                  >
                    <Icon name="next" size={14} /> Next
                  </button>
                  <button
                    type="button"
                    className="btn small"
                    title="Add to the end"
                    onClick={() => enqueue.mutate({ id: track.id, next: false })}
                  >
                    <Icon name="plus" size={14} />
                    <span className="visually-hidden">Add to the end</span>
                  </button>
                </>
              )}
            />
          </Card>
        )}
      </div>
    </>
  );
}
