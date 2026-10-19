// SPDX-License-Identifier: AGPL-3.0-or-later

// The buffer zone: uploads, and new files waiting to be imported, with
// their identification, flags and the path the layout gives them.

import { useQuery, useQueryClient } from "@tanstack/react-query";
import { type ReactNode, useCallback, useEffect, useRef, useState } from "react";
import { type Schemas, api, errorMessage, unwrap } from "../api";
import { TaskState, useTask } from "../components/Task";
import {
  Banner,
  Card,
  ConfirmButton,
  Empty,
  ErrorView,
  Icon,
  Loading,
  PageHead,
  useToast,
} from "../components/ui";
import { type PlainKey, rich, t } from "../i18n";
import { ago, bytes, duration } from "../lib/format";
import { Link } from "../lib/router";
import { useAction, useCan } from "../lib/session";
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
};

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

function DropZone({ onFiles, buffer }: { onFiles: (picked: Picked[]) => void; buffer: Schemas["Buffer"] | undefined }) {
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

function Uploads({ state }: { state: ReturnType<typeof useUploads> }) {
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
                  {upload.state === "sending" || upload.state === "waiting" ? (
                    <div className="progress" title={t("buffer.sentOf", { sent: bytes(upload.sent), size: bytes(upload.size) })}>
                      <div style={{ width: `${upload.size ? (100 * upload.sent) / upload.size : 0}%` }} />
                    </div>
                  ) : upload.state === "done" ? (
                    <span className="good">{t("buffer.sent")}</span>
                  ) : upload.state === "cancelled" ? (
                    <span className="muted">{t("buffer.cancelled")}</span>
                  ) : (
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
  const importing = taskId !== null && task.data?.status !== "done" && task.data?.status !== "failed";
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
      <div className="stack">
        {canEdit && <DropZone onFiles={onFiles} buffer={buffer.data} />}
        <Uploads state={uploads} />
        {importing && (
          <Banner kind="warn">
            {t("buffer.importing")} <TaskState task={task.data} />
          </Banner>
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
          <Group key={group.id} group={group} selected={selected} onToggle={toggle} canEdit={canEdit} />
        ))}
      </div>
    </>
  );
}

function Group({
  group,
  selected,
  onToggle,
  canEdit,
}: {
  group: StagedGroup;
  selected: Set<number>;
  onToggle: (ids: number[], on: boolean) => void;
  canEdit: boolean;
}) {
  const [open, setOpen] = useState<number | null>(null);
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
                <span className="visually-hidden">{t("buffer.details")}</span>
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
                onToggle={() => onToggle([file.id], !selected.has(file.id))}
                open={open === file.id}
                onOpen={() => setOpen(open === file.id ? null : file.id)}
              />
            ))}
          </tbody>
        </table>
      </div>
    </Card>
  );
}

function tagLine(tags: Schemas["StagedTags"] | null | undefined): string {
  if (!tags) {
    return "";
  }
  const name = [tags.artist, tags.title].filter(Boolean).join(" – ");
  return [name, tags.album].filter(Boolean).join(" · ");
}

function State({ file }: { file: StagedFile }) {
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
  for (const flag of file.flags) {
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
  onToggle,
  open,
  onOpen,
}: {
  file: StagedFile;
  canEdit: boolean;
  selected: boolean;
  onToggle: () => void;
  open: boolean;
  onOpen: () => void;
}) {
  const shown = file.tags ?? file.own_tags;
  return (
    <>
      <tr>
        {canEdit && (
          <td>
            <input
              type="checkbox"
              aria-label={t("library.select", { name: file.name })}
              checked={selected}
              disabled={file.status === "importing"}
              onChange={onToggle}
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
        </td>
        <td className="hide-narrow mono small" style={{ overflowWrap: "anywhere", maxWidth: "22rem" }}>
          {file.planned_path ?? <span className="muted">–</span>}
        </td>
        <td>
          <State file={file} />
        </td>
        <td className="actions">
          <button
            type="button"
            className="btn ghost small"
            aria-expanded={open}
            onClick={onOpen}
            disabled={file.status === "receiving"}
          >
            {open ? t("buffer.hide") : t("buffer.details")}
          </button>
        </td>
      </tr>
      {open && (
        <tr>
          <td colSpan={canEdit ? 5 : 4} style={{ background: "var(--surface-2)" }}>
            <Details file={file} canEdit={canEdit} />
          </td>
        </tr>
      )}
    </>
  );
}

const TAG_FIELDS: { key: keyof Schemas["StagedTags"]; label: PlainKey }[] = [
  { key: "title", label: "buffer.tag.title" },
  { key: "artist", label: "buffer.tag.artist" },
  { key: "album", label: "buffer.tag.album" },
  { key: "album_artist", label: "buffer.tag.albumArtist" },
  { key: "track", label: "buffer.tag.track" },
  { key: "disc", label: "buffer.tag.disc" },
  { key: "date", label: "buffer.tag.date" },
];

function Details({ file, canEdit }: { file: StagedFile; canEdit: boolean }) {
  const [cover, setCover] = useState(true);
  const choose = useAction(
    (body: Schemas["ChoiceRequest"]) =>
      unwrap(api.POST("/api/v1/staging/files/{id}/choice", { params: { path: { id: file.id } }, body })),
    { invalidate: [["buffer"]] },
  );
  const changing = file.status !== "ready" || choose.isPending;
  return (
    <div className="stack" style={{ padding: "0.5rem 0" }}>
      <div className="row" style={{ alignItems: "flex-start", gap: "1rem" }}>
        {file.release_id && cover && (
          <img
            className="cover-thumb"
            src={`https://coverartarchive.org/release/${file.release_id}/front-250`}
            alt={t("buffer.cover")}
            loading="lazy"
            onError={() => setCover(false)}
          />
        )}
        <div className="stack" style={{ flex: 1, minWidth: "14rem" }}>
          <div className="mono small" style={{ overflowWrap: "anywhere" }}>
            {file.name}
          </div>
          {file.status !== "identifying" && (
            <audio controls preload="none" src={`/api/v1/staging/files/${file.id}/audio`} style={{ width: "100%" }}>
              <track kind="captions" />
            </audio>
          )}
          {file.identification?.reason && <span className="warn small">{file.identification.reason}</span>}
          {file.error && <span className="bad small">{file.error}</span>}
          {file.duplicate_of_track_id && (
            <span className="small">
              {rich("buffer.like", {
                link: <Link to={`/library/${file.duplicate_of_track_id}`}>{t("buffer.likeLink")}</Link>,
              })}
            </span>
          )}
        </div>
      </div>
      <div className="table-wrap">
        <table className="table">
          <thead>
            <tr>
              <th>{t("buffer.tag")}</th>
              <th>{t("buffer.inFile")}</th>
              <th>{t("buffer.onceImported")}</th>
            </tr>
          </thead>
          <tbody>
            {TAG_FIELDS.map(({ key, label }) => {
              const own = file.own_tags?.[key];
              const applied = file.tags?.[key];
              return (
                <tr key={key}>
                  <td className="muted small">{t(label)}</td>
                  <td>{own ?? <span className="muted">–</span>}</td>
                  <td className={applied !== own ? "good" : undefined}>{applied ?? <span className="muted">–</span>}</td>
                </tr>
              );
            })}
          </tbody>
        </table>
      </div>
      {file.candidates.length > 0 && (
        <div className="table-wrap">
          <table className="table">
            <thead>
              <tr>
                <th>{t("buffer.candidate")}</th>
                <th className="hide-narrow">{t("review.date")}</th>
                <th className="hide-narrow">{t("review.country")}</th>
                <th>{t("buffer.tag.track")}</th>
                <th>{t("buffer.score")}</th>
                <th>
                  <span className="visually-hidden">{t("buffer.actions")}</span>
                </th>
              </tr>
            </thead>
            <tbody>
              {file.candidates.map((candidate) => {
                const picked = file.release_id === candidate.release_id;
                return (
                  <tr key={`${candidate.release_id}-${candidate.recording_id}-${candidate.disc}-${candidate.track}`}>
                    <td className="title-cell">
                      <a href={`https://musicbrainz.org/release/${candidate.release_id}`} target="_blank" rel="noreferrer">
                        {candidate.release_title ?? candidate.release_group_title ?? candidate.release_id}
                      </a>
                    </td>
                    <td className="hide-narrow">{candidate.date ?? "–"}</td>
                    <td className="hide-narrow">{candidate.country ?? "–"}</td>
                    <td className="num">
                      {candidate.disc && candidate.disc > 1 ? `${candidate.disc}-` : ""}
                      {candidate.track ?? "–"}
                    </td>
                    <td className="num">{t("buffer.percent", { score: Math.round(candidate.score * 100) })}</td>
                    <td className="actions">
                      {picked ? (
                        <span className="badge good">{t("buffer.chosen")}</span>
                      ) : (
                        canEdit && (
                          <button
                            type="button"
                            className="btn small"
                            disabled={changing}
                            onClick={() =>
                              choose.mutate({
                                kind: "release",
                                release_id: candidate.release_id,
                                recording_id: candidate.recording_id,
                              })
                            }
                          >
                            {t("buffer.use")}
                          </button>
                        )
                      )}
                    </td>
                  </tr>
                );
              })}
            </tbody>
          </table>
        </div>
      )}
      {canEdit && file.identification && file.identification.status !== "off" && file.choice?.kind !== "own" && (
        <div>
          <button type="button" className="btn small" disabled={changing} onClick={() => choose.mutate({ kind: "own" })}>
            {t("buffer.keepOwn")}
          </button>
        </div>
      )}
    </div>
  );
}
