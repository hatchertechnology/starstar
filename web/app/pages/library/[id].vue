<script setup lang="ts">
import type { Part, PartFile, Setup } from "~/types/api";
import { ApiRequestError, errorMessage } from "~/utils/api";
import { filesPath, partsPath, setupsPath } from "~/utils/paths";
const api = useApi(),
  session = useSession(),
  id = Number(useRoute().params.id);
const part = ref<Part | null>(null),
  error = ref<string | null>(null),
  actionError = ref<string | null>(null);
const selected = ref<number | null>(null),
  busy = ref(false),
  showRevision = ref(false),
  showSetup = ref(false),
  setupName = ref(""),
  note = ref(""),
  carry = ref<number[]>([]);
const uploadRole = ref<
    "source" | "render" | "other" | "project_3mf" | "slice" | null
  >(null),
  uploadSetup = ref<number | null>(null),
  uploadFile = ref<File | null>(null),
  profile = ref(""),
  conflict = ref(false);
type FailedUpload = {
  file: File;
  role: NonNullable<typeof uploadRole.value>;
  setup: number | null;
  modelRevision: number | undefined;
  profile: string;
};
const failedUpload = ref<FailedUpload | null>(null);
watch(
  [uploadFile, profile, uploadRole, uploadSetup, selected],
  () => {
    conflict.value = false;
    failedUpload.value = null;
  },
  { flush: "sync" },
);
const deletion = ref<{
  title: string;
  body: string;
  path: string;
  part?: boolean;
} | null>(null);
const latest = computed(() => part.value?.model_revisions[0]);
const revision = computed(
  () =>
    part.value?.model_revisions.find((r) => r.id === selected.value) ??
    latest.value,
);
useHead({
  title: computed(() => `${part.value?.name ?? "Library"} · starstar`),
});
async function load() {
  try {
    part.value = await api<Part>(partsPath(id));
    error.value = null;
  } catch (e) {
    error.value = errorMessage(e);
  }
}
onMounted(load);
function openRevision() {
  if (busy.value) return;
  carry.value = [];
  note.value = "";
  actionError.value = null;
  showRevision.value = true;
}
async function write(path: string, json: unknown) {
  if (busy.value) return;
  busy.value = true;
  actionError.value = null;
  try {
    await api(path, { method: "POST", json });
    showRevision.value = false;
    showSetup.value = false;
    await load();
  } catch (e) {
    actionError.value = errorMessage(e);
  } finally {
    busy.value = false;
  }
}
async function patch(path: string, json: unknown) {
  if (busy.value) return;
  busy.value = true;
  actionError.value = null;
  try {
    await api(path, { method: "PATCH", json });
    await load();
  } catch (e) {
    actionError.value = errorMessage(e);
  } finally {
    busy.value = false;
  }
}
function startUpload(
  role: typeof uploadRole.value,
  setup: number | null = null,
) {
  if (busy.value) return;
  uploadRole.value = role;
  uploadSetup.value = setup;
  uploadFile.value = null;
  profile.value = "";
  conflict.value = false;
  actionError.value = null;
}
async function upload(replace = false) {
  if (busy.value || !uploadFile.value || !uploadRole.value) return;
  const draft: FailedUpload =
    replace && failedUpload.value
      ? failedUpload.value
      : {
          file: uploadFile.value,
          role: uploadRole.value,
          setup: uploadSetup.value,
          modelRevision: revision.value?.id,
          profile: profile.value,
        };
  if (replace && !failedUpload.value) return;
  busy.value = true;
  actionError.value = null;
  conflict.value = false;
  try {
    await api(partsPath(id, "files"), {
      method: "POST",
      body: draft.file,
      query: {
        role: draft.role,
        setup: draft.setup === null ? undefined : String(draft.setup),
        name: draft.file.name,
        model_revision:
          draft.setup === null &&
          draft.role !== "other" &&
          draft.modelRevision !== undefined
            ? String(draft.modelRevision)
            : undefined,
        profile: draft.profile || undefined,
        replace: replace || undefined,
      },
    });
    uploadRole.value = null;
    failedUpload.value = null;
    await load();
  } catch (e) {
    actionError.value = errorMessage(e);
    conflict.value = e instanceof ApiRequestError && e.status === 409;
    failedUpload.value = conflict.value ? draft : null;
  } finally {
    busy.value = false;
  }
}
function askDelete(title: string, body: string, path: string, isPart = false) {
  if (busy.value) return;
  actionError.value = null;
  deletion.value = { title, body, path, part: isPart };
}
function deleteSetup(s: Setup) {
  const count = s.revisions.reduce(
    (n, r) => n + r.slices.length + (r.project ? 1 : 0),
    0,
  );
  askDelete(
    `Delete ${s.name}?`,
    `This removes ${s.revisions.length} revision(s) and ${count} file(s).`,
    setupsPath(s.id),
  );
}
function deleteFile(f: PartFile) {
  askDelete(`Delete ${f.name}?`, "This removes 1 file.", filesPath(f.id));
}
function deletePart() {
  if (!part.value) return;
  const p = part.value;
  const revisions =
    p.model_revisions.length +
    p.setups.reduce((n, s) => n + s.revisions.length, 0);
  const files =
    p.other_files.length +
    p.model_revisions.reduce((n, r) => n + r.files.length, 0) +
    p.setups.reduce(
      (n, s) =>
        n +
        s.revisions.reduce(
          (m, r) => m + r.slices.length + (r.project ? 1 : 0),
          0,
        ),
      0,
    );
  askDelete(
    `Delete ${p.name}?`,
    `This removes ${p.setups.length} setup(s), ${revisions} revision(s) and ${files} file(s).`,
    partsPath(id),
    true,
  );
}
async function remove() {
  if (!deletion.value || busy.value) return;
  busy.value = true;
  actionError.value = null;
  try {
    const d = deletion.value;
    await api(d.path, { method: "DELETE" });
    deletion.value = null;
    if (d.part) await navigateTo("/library");
    else await load();
  } catch (e) {
    actionError.value = errorMessage(e);
  } finally {
    busy.value = false;
  }
}
</script>
<template>
  <section>
    <p><NuxtLink to="/library">← Library</NuxtLink></p>
    <Notice v-if="error" kind="error">{{ error }}</Notice
    ><template v-if="part"
      ><header>
        <img
          v-if="part.preview_file_id !== null"
          :src="filesPath(part.preview_file_id, 'preview')"
          alt=""
          class="preview"
        />
        <p v-else class="muted">No preview available</p>
        <h1>{{ part.name }}</h1>
        <p>{{ part.notes }}</p>
        <button
          v-if="session.can('admin')"
          class="btn btn-secondary"
          :disabled="busy"
          @click="deletePart"
        >
          Delete part
        </button>
      </header>
      <h2>Model revisions</h2>
      <label
        >Model revision
        <select v-model="selected" :disabled="busy">
          <option :value="null">Latest (rev {{ latest?.number }})</option>
          <option v-for="r in part.model_revisions" :key="r.id" :value="r.id">
            Rev {{ r.number }}
          </option>
        </select></label
      >
      <p v-if="revision?.note">{{ revision.note }}</p>
      <PartFiles
        :files="revision?.files ?? []"
        :admin="session.can('admin')"
        :busy="busy"
        @delete="deleteFile"
      />
      <div class="actions">
        <button
          v-if="session.can('operator')"
          class="btn btn-secondary"
          :disabled="busy"
          @click="openRevision"
        >
          New revision</button
        ><button
          v-if="session.can('admin') && revision"
          class="btn btn-secondary"
          :disabled="busy"
          @click="
            askDelete(
              `Delete model revision ${revision.number}?`,
              `This removes ${revision.files.length} file(s).`,
              `/api/model-revisions/${revision.id}`,
            )
          "
        >
          Delete model revision</button
        ><button
          v-if="session.can('operator')"
          class="btn btn-secondary"
          :disabled="busy"
          @click="startUpload('source')"
        >
          Upload source</button
        ><button
          v-if="session.can('operator')"
          class="btn btn-secondary"
          :disabled="busy"
          @click="startUpload('render')"
        >
          Upload render
        </button>
      </div>
      <h2>Other files</h2>
      <PartFiles
        :files="part.other_files"
        :admin="session.can('admin')"
        :busy="busy"
        @delete="deleteFile"
      /><button
        v-if="session.can('operator')"
        class="btn btn-secondary"
        :disabled="busy"
        @click="startUpload('other')"
      >
        Upload other file
      </button>
      <h2>Setups</h2>
      <button
        v-if="session.can('operator')"
        class="btn btn-secondary"
        :disabled="busy"
        @click="
          showSetup = true;
          actionError = null;
        "
      >
        New setup</button
      ><SetupCard
        v-for="s in part.setups"
        :key="s.id"
        :setup="s"
        :admin="session.can('admin')"
        :operator="session.can('operator')"
        :is-default="part.default_setup_id === s.id"
        :busy="busy"
        @upload="startUpload"
        @delete-setup="deleteSetup"
        @delete-file="deleteFile"
        @delete-revision="
          (rid, number, count) =>
            askDelete(
              `Delete setup revision ${number}?`,
              `This removes ${count} file(s).`,
              `/api/setup-revisions/${rid}`,
            )
        "
        @set-default="(sid) => patch(partsPath(id), { default_setup_id: sid })"
        @set-profile="
          (sid, value) =>
            patch(setupsPath(sid), { default_profile: value || null })
        "
      />
      <form
        v-if="showRevision"
        class="panel"
        @submit.prevent="
          write(partsPath(id, 'revisions'), { note: note || null, carry })
        "
      >
        <h2>New revision</h2>
        <p>
          Setups built from rev {{ latest?.number }} will stop matching
          automatically until they get a new revision built from the new
          revision.
        </p>
        <fieldset :disabled="busy">
          <legend>Carry files</legend>
          <label v-for="f in latest?.files" :key="f.id"
            ><input v-model="carry" type="checkbox" :value="f.id" />
            {{ f.name }}</label
          ><label>Note <input v-model="note" /></label>
        </fieldset>
        <button class="btn btn-primary" :disabled="busy">Create revision</button
        ><button
          type="button"
          class="btn btn-secondary"
          :disabled="busy"
          @click="showRevision = false"
        >
          Cancel
        </button>
      </form>
      <form
        v-if="showSetup"
        class="panel"
        @submit.prevent="write(partsPath(id, 'setups'), { name: setupName })"
      >
        <h2>New setup</h2>
        <label
          >Name <input v-model="setupName" :disabled="busy" required /></label
        ><button class="btn btn-primary" :disabled="busy">Create setup</button
        ><button
          type="button"
          class="btn btn-secondary"
          :disabled="busy"
          @click="showSetup = false"
        >
          Cancel
        </button>
      </form>
      <form v-if="uploadRole" class="panel" @submit.prevent="upload()">
        <h2>Upload {{ uploadRole }}</h2>
        <label
          >File
          <input
            type="file"
            :disabled="busy"
            required
            @change="
              uploadFile =
                ($event.target as HTMLInputElement).files?.[0] ?? null
            " /></label
        ><label v-if="uploadRole === 'slice'"
          >Profile <input v-model="profile" :disabled="busy" /></label
        ><button class="btn btn-primary" :disabled="busy || !uploadFile">
          Upload</button
        ><button
          v-if="conflict"
          type="button"
          class="btn btn-primary"
          :disabled="busy"
          @click="upload(true)"
        >
          Replace</button
        ><button
          type="button"
          class="btn btn-secondary"
          :disabled="busy"
          @click="uploadRole = null"
        >
          Cancel
        </button>
      </form>
      <Notice v-if="actionError && !deletion" kind="error">{{
        actionError
      }}</Notice></template
    ><ConfirmDialog
      :open="!!deletion"
      :title="deletion?.title ?? ''"
      :body="deletion?.body ?? ''"
      confirm-label="Delete"
      danger
      :busy="busy"
      :error="actionError"
      @confirm="remove"
      @cancel="!busy && (deletion = null)"
    />
  </section>
</template>
<style scoped>
.preview {
  max-width: 16rem;
  max-height: 16rem;
  object-fit: contain;
}
.actions {
  display: flex;
  flex-wrap: wrap;
  gap: var(--s2);
}
.panel {
  border: 1px solid #aaa;
  padding: var(--s4);
  margin-block: var(--s4);
}
.panel label {
  display: block;
}
</style>
