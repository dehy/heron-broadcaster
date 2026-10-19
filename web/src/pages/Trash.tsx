// SPDX-License-Identifier: AGPL-3.0-or-later

// Deleted tracks, until the trash is purged: restore them, or empty it.

import { keepPreviousData, useQuery, useQueryClient } from "@tanstack/react-query";
import { useEffect, useState } from "react";
import { api, unwrap } from "../api";
import { TaskState, useTask } from "../components/Task";
import { Banner, Card, ConfirmButton, Empty, ErrorView, Icon, Loading, PageHead, useToast } from "../components/ui";
import { rich, t } from "../i18n";
import { ago, dateTime, fullDate, trackName } from "../lib/format";
import { Link, setSearch, useLocation } from "../lib/router";
import { useAction, useCan } from "../lib/session";

const PAGE = 50;

export function Trash() {
  const { search } = useLocation();
  const page = Math.max(0, Number(search.get("page") ?? 0) || 0);
  const canRestore = useCan("operator");
  const isAdmin = useCan("admin");
  const client = useQueryClient();
  const toast = useToast();
  const trash = useQuery({
    queryKey: ["trash", page],
    queryFn: () => unwrap(api.GET("/api/v1/library/trash", { params: { query: { limit: PAGE, offset: page * PAGE } } })),
    placeholderData: keepPreviousData,
  });
  const [taskId, setTaskId] = useState<number | null>(null);
  const task = useTask(taskId);
  const restore = useAction(
    (batch: number) =>
      unwrap(api.POST("/api/v1/library/trash/{batch_id}/restore", { params: { path: { batch_id: batch } } })),
    { onSuccess: (queued) => setTaskId(queued.id) },
  );
  const empty = useAction(() => unwrap(api.POST("/api/v1/library/trash/empty")), {
    onSuccess: (queued) => setTaskId(queued.id),
  });

  useEffect(() => {
    const data = task.data;
    if (data?.status === "done") {
      if (data.purge) {
        toast(t("trash.purged", { count: data.purge.tracks }));
      } else if (data.report) {
        toast(
          data.report.skipped.length
            ? t("trash.restoredSkipped", { count: data.report.done, skipped: data.report.skipped.length })
            : t("trash.restored", { count: data.report.done }),
        );
      }
      void client.invalidateQueries({ queryKey: ["trash"] });
      void client.invalidateQueries({ queryKey: ["tracks"] });
      void client.invalidateQueries({ queryKey: ["library-batches"] });
    } else if (data?.status === "failed") {
      toast(data.error ?? t("trash.failed"), true);
    }
  }, [task.data, client, toast]);

  const busy = taskId !== null && task.data?.status !== "done" && task.data?.status !== "failed";
  const items = trash.data?.items ?? [];
  const total = trash.data?.total ?? 0;
  const pages = Math.max(1, Math.ceil(total / PAGE));
  const inBatch = (batch: number | null | undefined) => items.filter((item) => item.batch_id === batch).length;

  return (
    <>
      <PageHead
        title={t("nav.trash")}
        description={trash.data ? t("trash.description", { count: trash.data.trash_days }) : undefined}
        actions={
          isAdmin &&
          total > 0 && (
            <ConfirmButton
              className="btn danger"
              confirm={t("trash.empty")}
              disabled={busy}
              question={<p>{t("trash.emptyQuestion", { count: total })}</p>}
              onConfirm={() => empty.mutate(undefined)}
            >
              <Icon name="trash" size={14} /> {t("trash.empty")}
            </ConfirmButton>
          )
        }
      />
      {busy && (
        <Banner kind="warn">
          {t("review.working")} <TaskState task={task.data} />
        </Banner>
      )}
      <Card flush>
        {trash.isPending && <Loading />}
        {trash.isError && (
          <div className="card-body">
            <ErrorView error={trash.error} />
          </div>
        )}
        {trash.data && items.length === 0 && <Empty>{t("trash.nothing")}</Empty>}
        {items.length > 0 && (
          <div className="table-wrap">
            <table className="table">
              <thead>
                <tr>
                  <th>{t("trash.track")}</th>
                  <th className="hide-narrow">{t("trash.wasAt")}</th>
                  <th>{t("trash.deleted")}</th>
                  <th className="hide-narrow">{t("trash.purgeAt")}</th>
                  <th>
                    <span className="visually-hidden">{t("buffer.actions")}</span>
                  </th>
                </tr>
              </thead>
              <tbody>
                {items.map((item) => (
                  <tr key={item.track.id}>
                    <td className="title-cell">
                      <Link to={`/library/${item.track.id}`} className="ellipsis">
                        {trackName(item.track)}
                      </Link>
                    </td>
                    <td className="hide-narrow mono small" style={{ overflowWrap: "anywhere" }}>
                      {item.original_path}
                    </td>
                    <td className="small" title={dateTime(item.deleted_at_unix_ms)}>
                      {ago(item.deleted_at_unix_ms)}
                    </td>
                    <td className="hide-narrow small">{fullDate(item.purge_at_unix_ms)}</td>
                    <td className="actions">
                      {canRestore && item.batch_id && (
                        <ConfirmButton
                          className="btn small"
                          confirm={t("trash.restore")}
                          disabled={busy}
                          question={
                            <p>
                              {rich("trash.restoreQuestion", {
                                batch: item.batch_id,
                                count: inBatch(item.batch_id),
                              })}
                            </p>
                          }
                          onConfirm={() => restore.mutate(item.batch_id ?? 0)}
                        >
                          <Icon name="undo" size={14} /> {t("trash.restore")}
                        </ConfirmButton>
                      )}
                    </td>
                  </tr>
                ))}
              </tbody>
            </table>
          </div>
        )}
        {total > PAGE && (
          <div className="pager">
            <button
              type="button"
              className="btn small"
              disabled={page === 0}
              onClick={() => setSearch({ page: page - 1 || undefined })}
            >
              {t("library.previous")}
            </button>
            <span className="muted small num">
              {t("library.page", { page: page + 1, pages })} · {t("common.tracks", { count: total })}
            </span>
            <button
              type="button"
              className="btn small"
              disabled={page + 1 >= pages}
              onClick={() => setSearch({ page: page + 1 })}
            >
              {t("dashboard.next")}
            </button>
          </div>
        )}
      </Card>
    </>
  );
}
