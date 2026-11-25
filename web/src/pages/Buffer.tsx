// SPDX-License-Identifier: AGPL-3.0-or-later

// The buffer zone: uploads, and new files waiting to be imported, with
// their identification, flags and the path the layout gives them. Files
// are prepared here (categories, labels, mood, sound code) for the tracks
// they become.

import { useQuery, useQueryClient } from "@tanstack/react-query";
import { type ReactNode, createContext, useCallback, useContext, useEffect, useRef, useState } from "react";
import { type Schemas, api, errorMessage, unwrap } from "../api";
import {
  CategoryDialog,
  type Change,
  ChangeField,
  LabelChip,
  LabelsDialog,
  ProgrammingDialog,
  changeValue,
} from "../components/SelectionActions";
import { TagValueInput, suggestedField } from "../components/TagValueInput";
import { ReviewDialog, currentCandidate, isAccepted, isSure, toReview } from "./BufferReview";
import { TaskState, useTask } from "../components/Task";
import {
  Banner,
  Card,
  ConfirmButton,
  Empty,
  ErrorView,
  Icon,
  Loading,
  Modal,
  PageHead,
  useToast,
} from "../components/ui";
import { type PlainKey, rich, t } from "../i18n";
import { ago, bytes, duration } from "../lib/format";
import { useModule } from "../lib/modules";
import { Link } from "../lib/router";
import { useAction, useCan } from "../lib/session";
import { type RangeSelection, useRangeSelection } from "../lib/rangeSelection";
import {
  AUDIO_EXTENSIONS,
  type Picked,
  apiTransport,
  droppedFiles,
  groupName,
  isUploadable,
  uploadFile,
} from "../lib/upload";

type StagedFile = Schemas["StagedFile"];

/** Files another one of the buffer zone is a copy of: Heron flags only the later copy. */
const Copied = createContext<Set<number>>(new Set());
type StagedGroup = Schemas["StagedGroup"];

/** Files uploaded at the same time. */
const PARALLEL_UPLOADS = 2;

const FLAGS: Record<Schemas["StagedFlag"], { label: PlainKey; help: PlainKey }> = {
  duplicate_in_library: { label: "buffer.flag.inLibrary", help: "buffer.flag.inLibraryHelp" },
  duplicate_in_buffer: { label: "buffer.flag.twice", help: "buffer.flag.twiceHelp" },
  same_recording_in_library: { label: "buffer.flag.sameRecording", help: "buffer.flag.sameRecordingHelp" },
  undecodable: { label: "buffer.flag.unreadable", help: "buffer.flag.unreadableHelp" },
  too_short: { label: "buffer.flag.tooShort", help: "buffer.flag.tooShortHelp" },
  unsupported_format: { label: "buffer.flag.unsupported", help: "buffer.flag.unsupportedHelp" },
  broken_seek_table: { label: "buffer.flag.seekTable", help: "buffer.flag.seekTableHelp" },
  damaged_audio: { label: "buffer.flag.damaged", help: "buffer.flag.damagedHelp" },
  mixed_streams: { label: "buffer.flag.mixed", help: "buffer.flag.mixedHelp" },
};

/** Files that can still be prepared: not yet imported, not being imported. */
function isPreparable(file: StagedFile): boolean {
  return file.status === "receiving" || file.status === "identifying" || file.status === "ready";
}

type Dialog = "category" | "labels" | "programming" | "tags" | null;


/** Ready, identified (or identification off) and nothing to look at. */
function isClean(file: StagedFile): boolean {
  return (
    file.status === "ready" &&
    file.flags.length === 0 &&
    !file.error &&
    (file.choice !== null && file.choice !== undefined
      ? true
      : file.identification?.status !== "needs_review")
  );
}

// Uploads -------------------------------------------------------------------

interface Upload {
  key: number;
  name: string;
  size: number;
  sent: number;
  state: "waiting" | "sending" | "done" | "failed" | "cancelled";
  error?: string | undefined;
  file: File;
  groupId: number;
  controller: AbortController;
}

function useUploads(onComplete: () => void) {
  const [uploads, setUploads] = useState<Upload[]>([]);
  const next = useRef(1);
  const update = useCallback((key: number, change: Partial<Upload>) => {
    setUploads((current) => current.map((u) => (u.key === key ? { ...u, ...change } : u)));
  }, []);

  // Starts the next waiting uploads, a few at a time.
  useEffect(() => {
    const sending = uploads.filter((u) => u.state === "sending").length;
    const waiting = uploads.filter((u) => u.state === "waiting").slice(0, PARALLEL_UPLOADS - sending);
    for (const upload of waiting) {
      update(upload.key, { state: "sending" });
      uploadFile(apiTransport, upload.groupId, upload.file, upload.name, {
        signal: upload.controller.signal,
        onProgress: (sent) => update(upload.key, { sent }),
      })
        .then(() => {
          update(upload.key, { state: "done", sent: upload.size });
          onComplete();
        })
        .catch((error: unknown) => {
          if (upload.controller.signal.aborted) {
            update(upload.key, { state: "cancelled" });
          } else {
            update(upload.key, { state: "failed", error: errorMessage(error) });
          }
        });
    }
  }, [uploads, update, onComplete]);

  const start = useCallback(async (picked: Picked[]) => {
    const group = await unwrap(api.POST("/api/v1/staging/groups", { body: { name: groupName(picked) } }));
    setUploads((current) => [
      ...current.filter((u) => u.state !== "done"),
      ...picked.map((p) => ({
        key: next.current++,
        name: p.path,
        size: p.file.size,
        sent: 0,
        state: "waiting" as const,
        file: p.file,
        groupId: group.id,
        controller: new AbortController(),
      })),
    ]);
  }, []);
  const cancel = (key: number) => {
    const upload = uploads.find((u) => u.key === key);
    upload?.controller.abort();
    if (upload?.state === "waiting") {
      update(key, { state: "cancelled" });
    }
  };
  const retry = (key: number) =>
    update(key, { state: "waiting", sent: 0, error: undefined, controller: new AbortController() });
  const clear = () => setUploads((current) => current.filter((u) => u.state === "waiting" || u.state === "sending"));
  return { uploads, start, cancel, retry, clear };
}

function DropZone({ onFiles, buffer }: Readonly<{ onFiles: (picked: Picked[]) => void; buffer: Schemas["Buffer"] | undefined }>) {
  const [over, setOver] = useState(false);
  const files = useRef<HTMLInputElement>(null);
  const folder = useRef<HTMLInputElement>(null);
  const pick = (list: FileList | null) => {
    if (list) {
      onFiles(
        Array.from(list).map((file) => ({ file, path: file.webkitRelativePath || file.name })),
      );
    }
  };
  const lowOnSpace =
    buffer?.free_bytes !== null &&
    buffer?.free_bytes !== undefined &&
    buffer.free_bytes < buffer.min_free_bytes * 1.2;
  return (
    <div
      className={over ? "dropzone over" : "dropzone"}
      onDragOver={(event) => {
        event.preventDefault();
        setOver(true);
      }}
      onDragLeave={() => setOver(false)}
      onDrop={(event) => {
        event.preventDefault();
        setOver(false);
        void droppedFiles(event.dataTransfer.items).then(onFiles);
      }}
    >
      <Icon name="upload" size={28} />
      <p style={{ margin: "0.5rem 0" }}>{t("buffer.drop")}</p>
      <div className="row" style={{ justifyContent: "center" }}>
        <button type="button" className="btn small" onClick={() => files.current?.click()}>
          {t("buffer.chooseFiles")}
        </button>
        <button type="button" className="btn small" onClick={() => folder.current?.click()}>
          {t("buffer.chooseFolder")}
        </button>
      </div>
      <input
        ref={files}
        type="file"
        multiple
        hidden
        accept={AUDIO_EXTENSIONS.map((e) => `.${e}`).join(",")}
        onChange={(event) => {
          pick(event.target.files);
          event.target.value = "";
        }}
      />
      <input
        ref={folder}
        type="file"
        hidden
        {...{ webkitdirectory: "" }}
        onChange={(event) => {
          pick(event.target.files);
          event.target.value = "";
        }}
      />
      {buffer && (
        <p className={lowOnSpace ? "small warn" : "small muted"} style={{ margin: "0.5rem 0 0" }}>
          {AUDIO_EXTENSIONS.join(", ")} · {t("buffer.limit", { size: bytes(buffer.max_upload_bytes) })}
          {buffer.free_bytes !== null && buffer.free_bytes !== undefined && (
            <> · {t("buffer.free", { size: bytes(buffer.free_bytes) })}</>
          )}
        </p>
      )}
    </div>
  );
}

function Uploads({ state }: Readonly<{ state: ReturnType<typeof useUploads> }>) {
  const { uploads, cancel, retry, clear } = state;
  if (uploads.length === 0) {
    return null;
  }
  const total = uploads.reduce((sum, u) => sum + u.size, 0);
  const sent = uploads.reduce((sum, u) => sum + (u.state === "cancelled" ? u.size : u.sent), 0);
  const active = uploads.filter((u) => u.state === "waiting" || u.state === "sending").length;
  return (
    <Card
      title={active > 0 ? t("buffer.uploading", { count: active }) : t("buffer.uploads")}
      actions={
        active === 0 && (
          <button type="button" className="btn small ghost" onClick={clear}>
            {t("queuePage.clear")}
          </button>
        )
      }
      flush
    >
      <div className="card-body">
        <div className="progress" aria-label={t("buffer.progress")}>
          <div style={{ width: `${total ? (100 * sent) / total : 100}%` }} />
        </div>
      </div>
      <div className="table-wrap">
        <table className="table">
          <tbody>
            {uploads.map((upload) => (
              <tr key={upload.key}>
                <td className="mono small" style={{ overflowWrap: "anywhere" }}>
                  {upload.name}
                </td>
                <td className="num small" style={{ width: "9rem" }}>
                  {(upload.state === "sending" || upload.state === "waiting") && (
                    <div className="progress" title={t("buffer.sentOf", { sent: bytes(upload.sent), size: bytes(upload.size) })}>
                      <div style={{ width: `${upload.size ? (100 * upload.sent) / upload.size : 0}%` }} />
                    </div>
                  )}
                  {upload.state === "done" && <span className="good">{t("buffer.sent")}</span>}
                  {upload.state === "cancelled" && <span className="muted">{t("buffer.cancelled")}</span>}
                  {upload.state === "failed" && (
                    <span className="bad" title={upload.error}>
                      {t("buffer.uploadFailed")}
                    </span>
                  )}
                </td>
                <td className="small bad">{upload.state === "failed" && upload.error}</td>
                <td className="actions">
                  {(upload.state === "sending" || upload.state === "waiting") && (
                    <button
                      type="button"
                      className="btn ghost small icon"
                      aria-label={t("buffer.cancelUpload", { name: upload.name })}
                      title={t("common.cancel")}
                      onClick={() => cancel(upload.key)}
                    >
                      <Icon name="close" />
                    </button>
                  )}
                  {upload.state === "failed" && (
                    <button type="button" className="btn small" onClick={() => retry(upload.key)}>
                      {t("buffer.retry")}
                    </button>
                  )}
                </td>
              </tr>
            ))}
          </tbody>
        </table>
      </div>
    </Card>
  );
}

// The buffer ----------------------------------------------------------------

export function Buffer() {
  const canEdit = useCan("operator");
  const labelsOn = useModule("labels");
  const client = useQueryClient();
  const toast = useToast();
  const refresh = useCallback(() => void client.invalidateQueries({ queryKey: ["buffer"] }), [client]);
  const uploads = useUploads(refresh);
  const buffer = useQuery({
    queryKey: ["buffer"],
    queryFn: () => unwrap(api.GET("/api/v1/staging")),
    refetchInterval: (query) => {
      const files = query.state.data?.groups.flatMap((g) => g.files) ?? [];
      return files.some((f) => f.status !== "ready" && f.status !== "failed") ? 2000 : 15_000;
    },
  });
  const [selected, setSelected] = useState<Set<number>>(new Set());
  const seen = useRef(new Set<number>());
  const [taskId, setTaskId] = useState<number | null>(null);
  const task = useTask(taskId);
  const [dialog, setDialog] = useState<Dialog>(null);
  // The files reviewed one after another, and where it starts.
  const [reviewing, setReviewing] = useState<{ queue: number[]; start: number } | null>(null);
  // The last import, to find its tracks in the library.
  const [imported, setImported] = useState<{ batch: number; count: number } | null>(null);

  // Files that need no look are selected as they become ready.
  useEffect(() => {
    const files = buffer.data?.groups.flatMap((g) => g.files) ?? [];
    const fresh = files.filter((f) => isClean(f) && !seen.current.has(f.id));
    for (const file of files) {
      if (file.status === "ready") {
        seen.current.add(file.id);
      }
    }
    const present = new Set(files.map((f) => f.id));
    setSelected((current) => {
      const next = new Set([...current].filter((id) => present.has(id)));
      for (const file of fresh) {
        next.add(file.id);
      }
      return next;
    });
  }, [buffer.data]);

  useEffect(() => {
    if (task.data?.status === "done") {
      const report = task.data.report;
      toast(
        report?.skipped.length
          ? t("buffer.importedSkipped", { count: report.done, skipped: report.skipped.length })
          : t("buffer.imported", { count: report?.done ?? 0 }),
      );
      if (report && report.done > 0) {
        setImported({ batch: report.batch_id, count: report.done });
      }
      refresh();
      void client.invalidateQueries({ queryKey: ["tracks"] });
      void client.invalidateQueries({ queryKey: ["library-batches"] });
    } else if (task.data?.status === "failed") {
      toast(task.data.error ?? t("buffer.importFailed"), true);
      refresh();
    }
  }, [task.data?.status, task.data?.report, task.data?.error, toast, refresh, client]);

  const importFiles = useAction(
    (ids: number[]) => unwrap(api.POST("/api/v1/staging/import", { body: { file_ids: ids } })),
    {
      onSuccess: (queued) => {
        setTaskId(queued.id);
        setSelected(new Set());
      },
    },
  );
  const discard = useAction(
    (ids: number[]) => unwrap(api.POST("/api/v1/staging/discard", { body: { file_ids: ids } })),
    {
      success: (result) => t("buffer.discarded", { count: result.discarded }),
      invalidate: [["buffer"]],
      onSuccess: () => setSelected(new Set()),
    },
  );

  const onFiles = (picked: Picked[]) => {
    const audio = picked.filter((p) => isUploadable(p.path, AUDIO_EXTENSIONS));
    const left = picked.length - audio.length;
    if (audio.length === 0) {
      toast(left ? t("buffer.noAudio") : t("buffer.nothing"), true);
      return;
    }
    const tooBig = buffer.data ? audio.filter((p) => p.file.size > buffer.data.max_upload_bytes) : [];
    if (tooBig.length > 0) {
      toast(t("buffer.tooBig", { count: tooBig.length }), true);
    }
    const accepted = audio.filter((p) => !tooBig.includes(p));
    if (accepted.length > 0) {
      uploads.start(accepted).catch((error: unknown) => toast(errorMessage(error), true));
    }
    if (left > 0) {
      toast(t("buffer.notAudio", { count: left }));
    }
  };

  const groups = buffer.data?.groups ?? [];
  const files = groups.flatMap((g) => g.files);
  const importable = [...selected].filter((id) => files.find((f) => f.id === id)?.status === "ready");
  const preparable = [...selected].filter((id) => {
    const file = files.find((f) => f.id === id);
    return file !== undefined && isPreparable(file);
  });
  const importing = taskId !== null && task.data?.status !== "done" && task.data?.status !== "failed";
  const copied = new Set(files.flatMap((f) => (f.duplicate_of_file_id ? [f.duplicate_of_file_id] : [])));
  const reviewable = canEdit ? files.filter((f) => f.status === "ready").map((f) => f.id) : [];
  const waiting = files.filter(toReview);
  const sure = files.filter(isSure);
  const accepted = files.filter(isAccepted).map((f) => f.id);
  const review = (id?: number) => {
    const first = id ?? waiting[0]?.id;
    const at = first === undefined ? -1 : reviewable.indexOf(first);
    if (at >= 0) {
      setReviewing({ queue: reviewable, start: at });
    }
  };
  const acceptSure = useAction(
    async (list: StagedFile[]) => {
      for (const file of list) {
        const candidate = currentCandidate(file);
        if (candidate) {
          await unwrap(
            api.POST("/api/v1/staging/files/{id}/choice", {
              params: { path: { id: file.id } },
              body: { kind: "release", release_id: candidate.release_id, recording_id: candidate.recording_id },
            }),
          );
        }
      }
      return list.length;
    },
    { invalidate: [["buffer"]], success: (count) => t("buffer.review.acceptedSure", { count }) },
  );

  // Every file that can be selected, in the order shown, across the groups.
  const order = groups.flatMap((group) => group.files.filter((f) => f.status !== "importing").map((f) => f.id));
  const range = useRangeSelection(order, selected, setSelected, null);
  const toggle = (ids: number[], on: boolean) =>
    setSelected((current) => {
      const next = new Set(current);
      for (const id of ids) {
        if (on) {
          next.add(id);
        } else {
          next.delete(id);
        }
      }
      return next;
    });

  return (
    <>
      <PageHead
        title={t("nav.buffer")}
        description={t("buffer.description")}
      />
      <Copied.Provider value={copied}>
      <div className="stack">
        {canEdit && <DropZone onFiles={onFiles} buffer={buffer.data} />}
        <Uploads state={uploads} />
        {imported && !importing && (
          <output className="card card-head">
            <span>
              {rich("buffer.importedLink", {
                count: imported.count,
                link: (
                  <Link to={`/library?batch=${imported.batch}&sort=added`}>
                    {t("buffer.seeImported", { count: imported.count })}
                  </Link>
                ),
              })}
            </span>
            <button
              type="button"
              className="btn small ghost"
              aria-label={t("ui.close")}
              onClick={() => setImported(null)}
            >
              <Icon name="close" size={14} />
            </button>
          </output>
        )}
        {importing && (
          <Banner kind="warn">
            {t("buffer.importing")} <TaskState task={task.data} />
          </Banner>
        )}
        {canEdit && reviewable.length > 0 && !importing && (
          <div className="card card-head">
            <span>
              {waiting.length > 0
                ? t("buffer.review.waiting", { count: waiting.length })
                : t("buffer.review.allAccepted")}
            </span>
            <div className="row">
              {waiting.length > 0 && (
                <button type="button" className="btn small primary" onClick={() => review()}>
                  <Icon name="check" size={14} /> {t("buffer.review.action")}
                </button>
              )}
              <button
                type="button"
                className="btn small"
                disabled={sure.length === 0 || acceptSure.isPending}
                onClick={() => acceptSure.mutate(sure)}
              >
                {t("buffer.review.acceptSure", { count: sure.length })}
              </button>
              <button
                type="button"
                className="btn small primary"
                disabled={accepted.length === 0 || importFiles.isPending}
                onClick={() => importFiles.mutate(accepted)}
              >
                {t("buffer.review.importAccepted", { count: accepted.length })}
              </button>
            </div>
          </div>
        )}
        {canEdit && selected.size > 0 && (
          <div className="card-head card" style={{ background: "var(--accent-soft)" }}>
            <span>{t("buffer.selected", { count: selected.size })}</span>
            <div className="row">
              <button
                type="button"
                className="btn small primary"
                disabled={importable.length === 0 || importing || importFiles.isPending}
                onClick={() => importFiles.mutate(importable)}
              >
                <Icon name="check" size={14} /> {t("buffer.import", { count: importable.length })}
              </button>
              <button
                type="button"
                className="btn small"
                disabled={preparable.length === 0}
                onClick={() => setDialog("category")}
              >
                {t("library.categoryAction")}
              </button>
              {labelsOn && (
                <button
                  type="button"
                  className="btn small"
                  disabled={preparable.length === 0}
                  onClick={() => setDialog("labels")}
                >
                  {t("library.labelsAction")}
                </button>
              )}
              <button
                type="button"
                className="btn small"
                disabled={preparable.length === 0}
                onClick={() => setDialog("programming")}
              >
                {t("library.programmingAction")}
              </button>
              <button
                type="button"
                className="btn small"
                disabled={preparable.length === 0}
                onClick={() => setDialog("tags")}
              >
                <Icon name="edit" size={14} /> {t("library.tagsAction")}
              </button>
              <ConfirmButton
                className="btn small danger"
                confirm={t("buffer.discardConfirm", { count: selected.size })}
                question={<p>{t("buffer.discardQuestion", { count: selected.size })}</p>}
                onConfirm={() => discard.mutate([...selected])}
              >
                <Icon name="trash" size={14} /> {t("buffer.discard")}
              </ConfirmButton>
              <button type="button" className="btn small ghost" onClick={() => setSelected(new Set())}>
                {t("queuePage.clear")}
              </button>
            </div>
          </div>
        )}
        {buffer.isPending && <Loading />}
        {buffer.isError && <ErrorView error={buffer.error} />}
        {buffer.data && groups.length === 0 && (
          <Card>
            <Empty>
              {t("buffer.empty")}
            </Empty>
          </Card>
        )}
        {groups.map((group) => (
          <Group
            key={group.id}
            group={group}
            selected={selected}
            onToggle={toggle}
            range={range}
            canEdit={canEdit}
            onFill={review}
          />
        ))}
      </div>
      </Copied.Provider>
      <CategoryDialog
        open={dialog === "category"}
        onClose={() => setDialog(null)}
        files={preparable}
        tracks={preparable.length}
      />
      <LabelsDialog
        open={dialog === "labels"}
        onClose={() => setDialog(null)}
        files={preparable}
        tracks={preparable.length}
      />
      <ProgrammingDialog
        open={dialog === "programming"}
        onClose={() => setDialog(null)}
        files={preparable}
        tracks={preparable.length}
      />
      <TagsDialog open={dialog === "tags"} onClose={() => setDialog(null)} files={preparable} />
      {reviewing && (
        <ReviewDialog
          key={`${reviewing.queue.join(",")}@${reviewing.start}`}
          queue={reviewing.queue}
          start={reviewing.start}
          files={files}
          onClose={() => setReviewing(null)}
        />
      )}
    </>
  );
}

function Group({
  group,
  selected,
  onToggle,
  range,
  canEdit,
  onFill,
}: Readonly<{
  group: StagedGroup;
  selected: Set<number>;
  onToggle: (ids: number[], on: boolean) => void;
  range: RangeSelection<number>;
  canEdit: boolean;
  onFill: (id: number) => void;
}>) {
  const discard = useAction(
    (ids: number[]) => unwrap(api.POST("/api/v1/staging/discard", { body: { file_ids: ids } })),
    { success: (result) => t("buffer.discarded", { count: result.discarded }), invalidate: [["buffer"]] },
  );
  const selectable = group.files.filter((f) => f.status !== "importing").map((f) => f.id);
  const all = selectable.length > 0 && selectable.every((id) => selected.has(id));
  const ready = group.files.filter((f) => f.status === "ready").length;
  const flagged = group.files.filter((f) => f.flags.length > 0).length;
  const review = group.files.filter((f) => f.status === "ready" && !isClean(f)).length;
  return (
    <Card
      title={
        <div style={{ minWidth: 0 }}>
          <h2 className="ellipsis">{group.name}</h2>
          <div className="muted small">
            {group.source === "incoming"
              ? t("buffer.fromIncoming", { ago: ago(group.created_at_unix_ms) })
              : t("buffer.uploadedBy", { name: group.created_by ?? "?", ago: ago(group.created_at_unix_ms) })}{" "}
            · {t("buffer.groupFiles", { count: group.files.length, ready })}
            {review > 0 && <span className="warn">, {t("buffer.toLookAt", { count: review })}</span>}
            {flagged > 0 && <span className="bad">, {t("buffer.flagged", { count: flagged })}</span>}
          </div>
          <GroupPreparation files={group.files} />
        </div>
      }
      flush
    >
      <div className="table-wrap">
        <table className="table">
          <thead>
            <tr>
              {canEdit && (
                <th style={{ width: "2rem" }}>
                  <input
                    type="checkbox"
                    aria-label={t("buffer.selectGroup", { name: group.name })}
                    checked={all}
                    onChange={() => onToggle(selectable, !all)}
                  />
                </th>
              )}
              <th>{t("buffer.file")}</th>
              <th className="hide-narrow">{t("buffer.goesTo")}</th>
              <th>{t("buffer.state")}</th>
              <th>
                <span className="visually-hidden">{t("buffer.actions")}</span>
              </th>
            </tr>
          </thead>
          <tbody>
            {group.files.map((file) => (
              <FileRows
                key={file.id}
                file={file}
                canEdit={canEdit}
                selected={selected.has(file.id)}
                range={range}
                onFill={() => onFill(file.id)}
                onDiscard={() => discard.mutate([file.id])}
              />
            ))}
          </tbody>
        </table>
      </div>
    </Card>
  );
}

/** The categories and labels the files of a group were given, with how
 * many of them have each when not all do. */
function GroupPreparation({ files }: Readonly<{ files: StagedFile[] }>) {
  const labelsOn = useModule("labels");
  const counts = new Map<string, { chip: { name: string; color?: string | null }; count: number }>();
  for (const file of files) {
    for (const category of file.categories) {
      const key = `c${category.category_id}`;
      const entry = counts.get(key) ?? { chip: category, count: 0 };
      entry.count += 1;
      counts.set(key, entry);
    }
    for (const label of labelsOn ? file.labels : []) {
      const key = `l${label.id}`;
      const entry = counts.get(key) ?? { chip: label, count: 0 };
      entry.count += 1;
      counts.set(key, entry);
    }
  }
  if (counts.size === 0) {
    return null;
  }
  return (
    <div className="chips" style={{ marginTop: "0.25rem" }}>
      {[...counts.entries()].map(([key, { chip, count }]) => (
        <LabelChip
          key={key}
          label={{
            name: count === files.length ? chip.name : t("buffer.someFiles", { name: chip.name, count, total: files.length }),
            color: chip.color,
          }}
        />
      ))}
    </div>
  );
}

/** What a file was prepared with. */
function Preparation({ file }: Readonly<{ file: StagedFile }>) {
  const labels = useModule("labels") ? file.labels : [];
  const programming = [
    file.mood && t("buffer.moodValue", { value: file.mood }),
    file.sound_code && t("buffer.soundCodeValue", { value: file.sound_code }),
  ]
    .filter(Boolean)
    .join(" · ");
  if (file.categories.length === 0 && labels.length === 0 && !programming) {
    return null;
  }
  return (
    <div className="chips small" style={{ marginTop: "0.25rem" }}>
      {file.categories.map((category) => (
        <LabelChip
          key={`c${category.category_id}`}
          label={{
            name: category.weight === 1 ? category.name : `${category.name} ×${category.weight}`,
            color: category.color,
          }}
        />
      ))}
      {labels.map((label) => (
        <LabelChip key={`l${label.id}`} label={label} />
      ))}
      {programming && <span className="muted">{programming}</span>}
    </div>
  );
}

function tagLine(tags: Schemas["StagedTags"] | null | undefined): string {
  if (!tags) {
    return "";
  }
  const name = [tags.artist, tags.title].filter(Boolean).join(" – ");
  return [name, tags.album].filter(Boolean).join(" · ");
}

function State({ file }: Readonly<{ file: StagedFile }>) {
  const copied = useContext(Copied);
  const badges: ReactNode[] = [];
  switch (file.status) {
    case "receiving":
      return (
        <div className="progress" title={t("buffer.receiving")} style={{ minWidth: "5rem" }}>
          <div style={{ width: `${file.size ? (100 * (file.received ?? 0)) / file.size : 0}%` }} />
        </div>
      );
    case "identifying":
      return (
        <span className="row small muted" style={{ flexWrap: "nowrap" }}>
          <span className="spinner" /> {file.identification ? t("buffer.updating") : t("buffer.identifying")}
        </span>
      );
    case "importing":
      return (
        <span className="row small muted" style={{ flexWrap: "nowrap" }}>
          <span className="spinner" /> {t("buffer.importing")}
        </span>
      );
    case "failed":
      return <span className="badge bad" title={file.error ?? undefined}>{t("buffer.failed")}</span>;
    default:
      break;
  }
  if (file.choice?.kind === "own") {
    badges.push(<span key="id" className="badge">{t("buffer.ownTags")}</span>);
  } else if (file.choice?.kind === "release") {
    badges.push(<span key="id" className="badge good">{t("buffer.releasePicked")}</span>);
  } else if (file.identification?.status === "identified") {
    badges.push(
      <span key="id" className="badge good" title={t("buffer.identifiedHelp")}>
        {file.identification.score
          ? t("buffer.identifiedScore", { score: Math.round(file.identification.score * 100) })
          : t("buffer.identified")}
      </span>,
    );
  } else if (file.identification?.status === "needs_review") {
    badges.push(
      <span key="id" className="badge warn" title={file.identification.reason ?? undefined}>
        {t("buffer.needsReview")}
      </span>,
    );
  } else {
    badges.push(<span key="id" className="badge">{t("buffer.ownTags")}</span>);
  }
  const flags: Schemas["StagedFlag"][] =
    copied.has(file.id) && !file.flags.includes("duplicate_in_buffer") ? [...file.flags, "duplicate_in_buffer"] : file.flags;
  for (const flag of flags) {
    badges.push(
      <span key={flag} className="badge bad" title={t(FLAGS[flag].help)}>
        {t(FLAGS[flag].label)}
      </span>,
    );
  }
  if (file.error) {
    badges.push(
      <span key="error" className="badge bad" title={file.error}>
        {t("buffer.error")}
      </span>,
    );
  }
  return <div className="row" style={{ gap: "0.25rem" }}>{badges}</div>;
}

function FileRows({
  file,
  canEdit,
  selected,
  range,
  onFill,
  onDiscard,
}: Readonly<{
  file: StagedFile;
  canEdit: boolean;
  selected: boolean;
  range: RangeSelection<number>;
  onFill: () => void;
  onDiscard: () => void;
}>) {
  const shown = file.tags ?? file.own_tags;
  return (
    <>
      <tr onClickCapture={canEdit && file.status !== "importing" ? range.row(file.id) : undefined}>
        {canEdit && (
          <td>
            <input
              type="checkbox"
              aria-label={t("library.select", { name: file.name })}
              checked={selected}
              disabled={file.status === "importing"}
              {...range.checkbox(file.id)}
            />
          </td>
        )}
        <td className="title-cell" style={{ maxWidth: "22rem" }}>
          <div className="ellipsis" title={file.name}>
            {shown?.title ?? file.name.split("/").pop()}
          </div>
          <div className="muted small ellipsis">
            {tagLine(shown) || file.name} · {duration(file.duration_ms)} · {bytes(file.size)}
          </div>
          <Preparation file={file} />
        </td>
        <td className="hide-narrow mono small" style={{ overflowWrap: "anywhere", maxWidth: "22rem" }}>
          {file.planned_path ?? <span className="muted">–</span>}
        </td>
        <td>
          <State file={file} />
        </td>
        <td className="actions">
          {canEdit && file.status === "ready" && (
            <>
              <ConfirmButton
                className="btn small ghost"
                title={t("buffer.discard")}
                confirm={t("buffer.discardConfirm", { count: 1 })}
                question={<p>{t("buffer.discardQuestion", { count: 1 })}</p>}
                onConfirm={onDiscard}
              >
                <Icon name="trash" size={14} />
              </ConfirmButton>
              <button type="button" className={toReview(file) ? "btn small primary" : "btn small"} onClick={onFill}>
                {toReview(file) ? t("buffer.review.action") : t("buffer.review.change")}
              </button>
            </>
          )}
        </td>
      </tr>
    </>
  );
}

type TagField = Schemas["TagField"];

/** Fields edited in the buffer zone. */
const EDITABLE: { field: TagField; key: keyof Schemas["StagedTags"]; label: PlainKey; number?: boolean }[] = [
  { field: "title", key: "title", label: "buffer.tag.title" },
  { field: "artist", key: "artist", label: "buffer.tag.artist" },
  { field: "album", key: "album", label: "buffer.tag.album" },
  { field: "album_artist", key: "album_artist", label: "buffer.tag.albumArtist" },
  { field: "year", key: "date", label: "buffer.tag.year", number: true },
  { field: "track", key: "track", label: "buffer.tag.track", number: true },
  { field: "disc", key: "disc", label: "buffer.tag.disc", number: true },
];

/** Edits the tags files are imported with, on top of their identification. */
function TagsDialog({
  open,
  onClose,
  files,
  current,
}: Readonly<{
  open: boolean;
  onClose: () => void;
  files: number[];
  /** The tags of a single file, to start from. */
  current?: Schemas["StagedTags"] | null | undefined;
}>) {
  const [changes, setChanges] = useState<Partial<Record<TagField, Change<string>>>>({});
  useEffect(() => {
    if (open) {
      setChanges({});
    }
  }, [open]);
  const apply = useAction(
    () => {
      const set: Record<string, string | null> = {};
      for (const [field, change] of Object.entries(changes)) {
        const value = change ? changeValue(change) : undefined;
        if (value !== undefined) {
          set[field] = value;
        }
      }
      return unwrap(api.POST("/api/v1/staging/files/tags", { body: { file_ids: files, set } }));
    },
    {
      success: (result) => t("buffer.tagsEdited", { count: result.files }),
      invalidate: [["buffer"]],
      onSuccess: onClose,
    },
  );
  const edited = Object.values(changes).some((change) => change && change.kind !== "keep");
  // The artist albums are proposed for: the one typed, else the file's.
  const typedArtist = changes.artist?.kind === "set" ? changes.artist.value : current?.artist;
  const invalid = EDITABLE.some(({ field, number }) => {
    const change = changes[field];
    return change?.kind === "set" && (!change.value.trim() || (number && (Number.isNaN(Number(change.value)) || Number(change.value) < 1)));
  });
  return (
    <Modal
      title={t("buffer.tagsTitle", { count: files.length })}
      open={open}
      onClose={onClose}
      wide
      footer={
        <>
          <button type="button" className="btn" onClick={onClose}>
            {t("common.cancel")}
          </button>
          <button
            type="button"
            className="btn primary"
            disabled={!edited || invalid || apply.isPending}
            onClick={() => apply.mutate(undefined)}
          >
            {t("selection.apply")}
          </button>
        </>
      }
    >
      <div className="stack">
        <p className="muted small" style={{ margin: 0 }}>
          {files.length > 1 ? t("buffer.tagsHelpMany") : t("buffer.tagsHelpOne")}
        </p>
        <div className="form-grid">
          {EDITABLE.map(({ field, key, label, number: numeric }) => {
            const value = current?.[key];
            const name = t(label);
            return (
              <ChangeField
                key={field}
                label={
                  value !== undefined && value !== null && value !== false
                    ? t("selection.currentValue", { field: name, value: String(value) })
                    : name
                }
                change={changes[field] ?? { kind: "keep" }}
                onChange={(change) => setChanges((all) => ({ ...all, [field]: change }))}
                renderInput={(text, set) => {
                  const suggested = suggestedField(field);
                  return suggested ? (
                    <TagValueInput field={suggested} value={text} onValue={set} artist={typedArtist} aria-label={name} />
                  ) : (
                    <input
                      className="input"
                      type={numeric ? "number" : "text"}
                      min={numeric ? 1 : undefined}
                      value={text}
                      aria-label={name}
                      onChange={(event) => set(event.target.value)}
                    />
                  );
                }}
              />
            );
          })}
        </div>
      </div>
    </Modal>
  );
}

