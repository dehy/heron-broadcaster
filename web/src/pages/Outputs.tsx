// SPDX-License-Identifier: AGPL-3.0-or-later

// Encoder profiles and outputs, with each output's live state.

import { useQuery } from "@tanstack/react-query";
import { type FormEvent, type ReactNode, useState } from "react";
import { type EncoderProfileView, type OutputView, type Schemas, api, done, unwrap } from "../api";
import {
  Banner,
  Card,
  ConfirmButton,
  Empty,
  ErrorView,
  Field,
  Icon,
  Loading,
  Modal,
  NumberInput,
  PageHead,
} from "../components/ui";
import { bytes, clockTime } from "../lib/format";
import { useLive } from "../lib/live";
import { useAction, useCan } from "../lib/session";
import { OutputState } from "./Dashboard";

type Encoder = Schemas["EncoderKind"];
type Kind = Schemas["OutputKind"];

// Built in so far; the others arrive in M10 and the engine refuses them.
const ENCODERS: { value: Encoder; label: string; available: boolean }[] = [
  { value: "mp3", label: "MP3 (LAME)", available: true },
  { value: "aac", label: "AAC-LC", available: false },
  { value: "opus", label: "Opus", available: false },
  { value: "vorbis", label: "Vorbis", available: false },
  { value: "external_ffmpeg", label: "External FFmpeg", available: false },
];

const KINDS: { value: Kind; label: string; available: boolean }[] = [
  { value: "icecast", label: "Icecast", available: true },
  { value: "shoutcast_v1", label: "Shoutcast v1", available: false },
  { value: "shoutcast_v2", label: "Shoutcast v2", available: false },
  { value: "hls", label: "HLS", available: false },
  { value: "file", label: "File", available: false },
  { value: "pipe", label: "Pipe", available: false },
];

const MP3_BITRATES = [32, 40, 48, 56, 64, 80, 96, 112, 128, 160, 192, 224, 256, 320];
const SAMPLE_RATES = [22_050, 32_000, 44_100, 48_000];

export function Outputs() {
  const isAdmin = useCan("admin");
  const outputs = useQuery({ queryKey: ["outputs"], queryFn: () => unwrap(api.GET("/api/v1/outputs")) });
  const profiles = useQuery({
    queryKey: ["encoder-profiles"],
    queryFn: () => unwrap(api.GET("/api/v1/encoder-profiles")),
  });
  const live = useLive((state) => state.player?.outputs);
  const [editingOutput, setEditingOutput] = useState<OutputView | "new" | null>(null);
  const [editingProfile, setEditingProfile] = useState<EncoderProfileView | "new" | null>(null);
  const removeOutput = useAction(
    (id: number) => done(api.DELETE("/api/v1/outputs/{id}", { params: { path: { id } } })),
    { success: "Output deleted", invalidate: [["outputs"], ["encoder-profiles"]] },
  );
  const removeProfile = useAction(
    (id: number) => done(api.DELETE("/api/v1/encoder-profiles/{id}", { params: { path: { id } } })),
    { success: "Profile deleted", invalidate: [["encoder-profiles"]] },
  );
  const toggle = useAction(
    (output: OutputView) =>
      unwrap(
        api.PUT("/api/v1/outputs/{id}", {
          params: { path: { id: output.id } },
          body: {
            name: output.name,
            kind: output.kind,
            encoder_profile_id: output.encoder_profile_id,
            config: output.config,
            metadata_enabled: output.metadata_enabled,
            enabled: !output.enabled,
          },
        }),
      ),
    { invalidate: [["outputs"]] },
  );

  if (outputs.isPending || profiles.isPending) {
    return <Loading />;
  }
  if (outputs.isError || profiles.isError) {
    return <ErrorView error={outputs.error ?? profiles.error} />;
  }
  const profileName = (id: number) => profiles.data.find((p) => p.id === id)?.name ?? `#${id}`;

  return (
    <>
      <PageHead
        title="Outputs"
        description="Where the station streams. Outputs sharing an encoder profile share one encoding."
        actions={
          isAdmin && (
            <button type="button" className="btn primary" onClick={() => setEditingOutput("new")}>
              <Icon name="plus" /> New output
            </button>
          )
        }
      />
      <Card title="Outputs" flush>
        {outputs.data.length === 0 ? (
          <Empty>No output yet: add the stream server listeners connect to.</Empty>
        ) : (
          <div className="table-wrap">
            <table className="table">
              <thead>
                <tr>
                  <th>Name</th>
                  <th className="hide-narrow">Server</th>
                  <th className="hide-narrow">Encoding</th>
                  <th>State</th>
                  <th>
                    <span className="visually-hidden">Actions</span>
                  </th>
                </tr>
              </thead>
              <tbody>
                {outputs.data.map((output) => {
                  const status = live?.find((s) => s.id === output.id) ?? output.status;
                  const config = output.config as Record<string, unknown>;
                  return (
                    <tr key={output.id}>
                      <td>
                        <div>{output.name}</div>
                        <div className="muted small">{KINDS.find((k) => k.value === output.kind)?.label}</div>
                      </td>
                      <td className="hide-narrow mono small">
                        {typeof config.host === "string"
                          ? `${config.host}:${String(config.port ?? 8000)}${String(config.mount ?? "")}`
                          : "–"}
                      </td>
                      <td className="hide-narrow">{profileName(output.encoder_profile_id)}</td>
                      <td>
                        <OutputState status={status} />
                        {status?.state === "connected" && status.connected_since_unix_ms && (
                          <div className="muted small">
                            since {clockTime(status.connected_since_unix_ms)}, {bytes(status.bytes_sent)}
                          </div>
                        )}
                        {status?.error && <div className="bad small">{status.error}</div>}
                      </td>
                      <td className="actions">
                        {isAdmin && (
                          <>
                            <button type="button" className="btn small" onClick={() => toggle.mutate(output)}>
                              {output.enabled ? "Disable" : "Enable"}
                            </button>
                            <button
                              type="button"
                              className="btn ghost small icon"
                              aria-label={`Edit ${output.name}`}
                              onClick={() => setEditingOutput(output)}
                            >
                              <Icon name="edit" />
                            </button>
                            <ConfirmButton
                              className="btn ghost small icon danger"
                              title={`Delete ${output.name}`}
                              confirm="Delete"
                              question={<p>Stop streaming to {output.name} and delete it?</p>}
                              onConfirm={() => removeOutput.mutate(output.id)}
                            >
                              <Icon name="trash" />
                            </ConfirmButton>
                          </>
                        )}
                      </td>
                    </tr>
                  );
                })}
              </tbody>
            </table>
          </div>
        )}
      </Card>

      <Card
        title="Encoder profiles"
        actions={
          isAdmin && (
            <button type="button" className="btn small" onClick={() => setEditingProfile("new")}>
              <Icon name="plus" size={14} /> New profile
            </button>
          )
        }
        flush
      >
        <table className="table">
          <thead>
            <tr>
              <th>Name</th>
              <th>Encoding</th>
              <th>Outputs</th>
              <th>
                <span className="visually-hidden">Actions</span>
              </th>
            </tr>
          </thead>
          <tbody>
            {profiles.data.map((profile) => (
              <tr key={profile.id}>
                <td>{profile.name}</td>
                <td className="small">{describeProfile(profile)}</td>
                <td className="num">{profile.outputs}</td>
                <td className="actions">
                  {isAdmin && (
                    <>
                      <button
                        type="button"
                        className="btn ghost small icon"
                        aria-label={`Edit ${profile.name}`}
                        onClick={() => setEditingProfile(profile)}
                      >
                        <Icon name="edit" />
                      </button>
                      <ConfirmButton
                        className="btn ghost small icon danger"
                        title={profile.outputs > 0 ? "Used by outputs" : `Delete ${profile.name}`}
                        disabled={profile.outputs > 0}
                        confirm="Delete"
                        question={<p>Delete the profile {profile.name}?</p>}
                        onConfirm={() => removeProfile.mutate(profile.id)}
                      >
                        <Icon name="trash" />
                      </ConfirmButton>
                    </>
                  )}
                </td>
              </tr>
            ))}
          </tbody>
        </table>
      </Card>

      {editingOutput && (
        <OutputDialog
          output={editingOutput === "new" ? null : editingOutput}
          profiles={profiles.data}
          onClose={() => setEditingOutput(null)}
        />
      )}
      {editingProfile && (
        <ProfileDialog
          profile={editingProfile === "new" ? null : editingProfile}
          onClose={() => setEditingProfile(null)}
        />
      )}
    </>
  );
}

function describeProfile(profile: EncoderProfileView): string {
  const encoder = ENCODERS.find((e) => e.value === profile.encoder)?.label ?? profile.encoder;
  const options = profile.options as Record<string, unknown>;
  const mode = typeof options.mode === "string" ? ` ${options.mode.toUpperCase()}` : "";
  const bitrate = profile.bitrate_kbps ? ` ${profile.bitrate_kbps} kbps` : "";
  const channels = profile.channels === 1 ? "mono" : "stereo";
  return `${encoder}${mode}${bitrate}, ${(profile.sample_rate / 1000).toFixed(1)} kHz ${channels}`;
}

function SaveFooter({ form, onClose, busy }: { form: string; onClose: () => void; busy: boolean }) {
  return (
    <>
      <button type="button" className="btn" onClick={onClose}>
        Cancel
      </button>
      <button type="submit" form={form} className="btn primary" disabled={busy}>
        Save
      </button>
    </>
  );
}

/** Text of an optional string setting. */
const text = (value: unknown) => (typeof value === "string" ? value : "");

function OutputDialog({
  output,
  profiles,
  onClose,
}: {
  output: OutputView | null;
  profiles: EncoderProfileView[];
  onClose: () => void;
}) {
  const initial = (output?.config ?? {}) as Record<string, unknown>;
  const [name, setName] = useState(output?.name ?? "");
  const [kind, setKind] = useState<Kind>(output?.kind ?? "icecast");
  const [profile, setProfile] = useState(
    output?.encoder_profile_id ?? profiles.find((p) => p.name === "MP3 128k")?.id ?? profiles[0]?.id ?? 0,
  );
  const [enabled, setEnabled] = useState(output?.enabled ?? true);
  const [metadata, setMetadata] = useState(output?.metadata_enabled ?? true);
  const [config, setConfig] = useState<Record<string, unknown>>({
    port: 8000,
    mount: "/live",
    username: "source",
    ...initial,
  });
  const [json, setJson] = useState(JSON.stringify(initial, null, 2));
  const [jsonError, setJsonError] = useState<string | null>(null);
  const passwordSet = output?.secrets_set.includes("password") ?? false;
  const set = (key: string, value: unknown) => setConfig((c) => ({ ...c, [key]: value }));

  const save = useAction(
    () => {
      let body: Record<string, unknown>;
      if (kind === "icecast") {
        body = {};
        for (const [key, value] of Object.entries(config)) {
          // Empty optional fields are left out; so is an unchanged password.
          if (value !== "" && value !== undefined && value !== null) {
            body[key] = value;
          }
        }
      } else {
        body = JSON.parse(json) as Record<string, unknown>;
      }
      const input = {
        name,
        kind,
        encoder_profile_id: profile,
        config: body as Record<string, never>,
        enabled,
        metadata_enabled: metadata,
      };
      return output
        ? unwrap(api.PUT("/api/v1/outputs/{id}", { params: { path: { id: output.id } }, body: input }))
        : unwrap(api.POST("/api/v1/outputs", { body: input }));
    },
    {
      success: "Saved: the engine applies it within seconds",
      invalidate: [["outputs"], ["encoder-profiles"], ["engine"]],
      onSuccess: onClose,
    },
  );

  const submit = (event: FormEvent) => {
    event.preventDefault();
    if (kind !== "icecast") {
      try {
        JSON.parse(json);
        setJsonError(null);
      } catch (error) {
        setJsonError(String(error));
        return;
      }
    }
    save.mutate(undefined);
  };

  return (
    <Modal
      title={output ? `Edit ${output.name}` : "New output"}
      open
      wide
      onClose={onClose}
      footer={<SaveFooter form="output-form" onClose={onClose} busy={save.isPending} />}
    >
      <form id="output-form" className="stack" onSubmit={submit}>
        <div className="form-grid">
          <Field label="Name">
            <input className="input" value={name} onChange={(e) => setName(e.target.value)} required />
          </Field>
          <Field label="Kind">
            <select className="input" value={kind} onChange={(e) => setKind(e.target.value as Kind)}>
              {KINDS.map((k) => (
                <option key={k.value} value={k.value} disabled={!k.available && k.value !== output?.kind}>
                  {k.label}
                  {k.available ? "" : " (coming later)"}
                </option>
              ))}
            </select>
          </Field>
          <Field label="Encoder profile">
            <select className="input" value={profile} onChange={(e) => setProfile(Number(e.target.value))}>
              {profiles.map((p) => (
                <option key={p.id} value={p.id}>
                  {p.name}
                </option>
              ))}
            </select>
          </Field>
        </div>
        <div className="row">
          <label className="check">
            <input type="checkbox" checked={enabled} onChange={(e) => setEnabled(e.target.checked)} /> Enabled
          </label>
          <label className="check">
            <input type="checkbox" checked={metadata} onChange={(e) => setMetadata(e.target.checked)} /> Send
            now-playing titles
          </label>
        </div>
        {kind === "icecast" ? (
          <fieldset>
            <legend>Icecast server</legend>
            <div className="form-grid">
              <Field label="Host">
                <input className="input" value={text(config.host)} onChange={(e) => set("host", e.target.value)} required />
              </Field>
              <Field label="Port">
                <NumberInput value={Number(config.port ?? 8000)} min={1} max={65535} step={1} onChange={(v) => set("port", v)} />
              </Field>
              <Field label="Mount point" hint="Starts with /">
                <input className="input" value={text(config.mount)} onChange={(e) => set("mount", e.target.value)} required pattern="/.*" />
              </Field>
              <Field label="Username">
                <input className="input" value={text(config.username)} onChange={(e) => set("username", e.target.value)} />
              </Field>
              <Field label="Password" hint={passwordSet ? "Leave empty to keep the current one" : undefined}>
                <input
                  className="input"
                  type="password"
                  autoComplete="new-password"
                  value={text(config.password)}
                  placeholder={passwordSet ? "••••••••" : ""}
                  onChange={(e) => set("password", e.target.value)}
                  required={!passwordSet}
                />
              </Field>
            </div>
            <div className="form-grid" style={{ marginTop: "0.9rem" }}>
              <Field label="Station name">
                <input className="input" value={text(config.name)} onChange={(e) => set("name", e.target.value)} />
              </Field>
              <Field label="Genre">
                <input className="input" value={text(config.genre)} onChange={(e) => set("genre", e.target.value)} />
              </Field>
              <Field label="Website">
                <input className="input" type="url" value={text(config.url)} onChange={(e) => set("url", e.target.value)} />
              </Field>
              <Field label="Description">
                <input className="input" value={text(config.description)} onChange={(e) => set("description", e.target.value)} />
              </Field>
            </div>
            <label className="check" style={{ marginTop: "0.75rem" }}>
              <input type="checkbox" checked={config.public === true} onChange={(e) => set("public", e.target.checked)} />
              List the stream in public directories
            </label>
          </fieldset>
        ) : (
          <Field label="Settings (JSON)" hint="See docs/outputs.md for this kind's settings">
            <textarea className="input" value={json} onChange={(e) => setJson(e.target.value)} rows={8} />
          </Field>
        )}
        {jsonError && <Banner kind="bad">{jsonError}</Banner>}
      </form>
    </Modal>
  );
}

function ProfileDialog({ profile, onClose }: { profile: EncoderProfileView | null; onClose: () => void }) {
  const options = (profile?.options ?? {}) as Record<string, unknown>;
  const [name, setName] = useState(profile?.name ?? "");
  const [encoder, setEncoder] = useState<Encoder>(profile?.encoder ?? "mp3");
  const [bitrate, setBitrate] = useState(profile?.bitrate_kbps ?? 128);
  const [sampleRate, setSampleRate] = useState(profile?.sample_rate ?? 44_100);
  const [channels, setChannels] = useState(profile?.channels ?? 2);
  const [mode, setMode] = useState(text(options.mode) || "cbr");
  const [quality, setQuality] = useState(Number(options.quality ?? 3));
  const [vbrQuality, setVbrQuality] = useState(Number(options.vbr_quality ?? 4));
  const save = useAction(
    () => {
      const body = {
        name,
        encoder,
        bitrate_kbps: bitrate,
        sample_rate: sampleRate,
        channels,
        options: (encoder === "mp3"
          ? mode === "vbr"
            ? { mode, quality, vbr_quality: vbrQuality }
            : { mode, quality }
          : options) as Record<string, never>,
      };
      return profile
        ? unwrap(api.PUT("/api/v1/encoder-profiles/{id}", { params: { path: { id: profile.id } }, body }))
        : unwrap(api.POST("/api/v1/encoder-profiles", { body }));
    },
    { success: "Saved", invalidate: [["encoder-profiles"], ["engine"]], onSuccess: onClose },
  );
  const row = (label: string, control: ReactNode, hint?: string) => (
    <Field label={label} hint={hint}>
      {control}
    </Field>
  );
  return (
    <Modal
      title={profile ? `Edit ${profile.name}` : "New encoder profile"}
      open
      onClose={onClose}
      footer={<SaveFooter form="profile-form" onClose={onClose} busy={save.isPending} />}
    >
      <form
        id="profile-form"
        className="stack"
        onSubmit={(event) => {
          event.preventDefault();
          save.mutate(undefined);
        }}
      >
        {profile && profile.outputs > 0 && (
          <Banner kind="warn">
            {profile.outputs} outputs use this profile: they reconnect with the new encoding.
          </Banner>
        )}
        <div className="form-grid">
          {row("Name", <input className="input" value={name} onChange={(e) => setName(e.target.value)} required />)}
          {row(
            "Encoder",
            <select className="input" value={encoder} onChange={(e) => setEncoder(e.target.value as Encoder)}>
              {ENCODERS.map((e) => (
                <option key={e.value} value={e.value} disabled={!e.available && e.value !== profile?.encoder}>
                  {e.label}
                  {e.available ? "" : " (coming later)"}
                </option>
              ))}
            </select>,
          )}
          {row(
            mode === "vbr" ? "Bitrate (VBR: ignored)" : "Bitrate",
            <select className="input" value={bitrate} onChange={(e) => setBitrate(Number(e.target.value))}>
              {MP3_BITRATES.map((b) => (
                <option key={b} value={b}>
                  {b} kbps
                </option>
              ))}
            </select>,
          )}
          {row(
            "Sample rate",
            <select className="input" value={sampleRate} onChange={(e) => setSampleRate(Number(e.target.value))}>
              {SAMPLE_RATES.map((rate) => (
                <option key={rate} value={rate}>
                  {(rate / 1000).toFixed(rate % 1000 ? 2 : 0)} kHz
                </option>
              ))}
            </select>,
          )}
          {row(
            "Channels",
            <select className="input" value={channels} onChange={(e) => setChannels(Number(e.target.value))}>
              <option value={2}>Stereo</option>
              <option value={1}>Mono</option>
            </select>,
          )}
        </div>
        {encoder === "mp3" && (
          <fieldset>
            <legend>MP3</legend>
            <div className="form-grid">
              {row(
                "Mode",
                <select className="input" value={mode} onChange={(e) => setMode(e.target.value)}>
                  <option value="cbr">Constant bitrate</option>
                  <option value="vbr">Variable bitrate</option>
                </select>,
                "CBR keeps the bitrate shown in directories exact",
              )}
              {row(
                "Algorithm quality",
                <NumberInput value={quality} min={0} max={9} step={1} onChange={setQuality} />,
                "0 is best and slowest",
              )}
              {mode === "vbr" &&
                row(
                  "VBR quality",
                  <NumberInput value={vbrQuality} min={0} max={9} step={1} onChange={setVbrQuality} />,
                  "0 is best",
                )}
            </div>
          </fieldset>
        )}
      </form>
    </Modal>
  );
}
