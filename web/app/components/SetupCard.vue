<script setup lang="ts">
import type { Setup, PartFile } from "~/types/api";
import { filesPath } from "~/utils/paths";
import { formatDate, formatDuration } from "~/utils/format";
const props = defineProps<{
  setup: Setup;
  admin: boolean;
  operator: boolean;
  isDefault: boolean;
  busy: boolean;
}>();
defineEmits<{
  upload: [role: "project_3mf" | "slice", setup: number];
  deleteSetup: [setup: Setup];
  deleteRevision: [id: number, number: number, count: number];
  deleteFile: [file: PartFile];
  setDefault: [id: number];
  setProfile: [id: number, profile: string];
}>();
const showReplaced = ref(false);
const profile = ref(props.setup.default_profile ?? "");
watch(
  () => props.setup.default_profile,
  (value) => {
    profile.value = value ?? "";
  },
);
</script>
<template>
  <article class="card">
    <h3>{{ setup.name }} <small v-if="isDefault">(default)</small></h3>
    <Notice v-if="setup.behind_model" kind="info">{{
      setup.behind_model
    }}</Notice>
    <div class="actions">
      <button
        v-if="operator"
        class="btn btn-secondary"
        :disabled="busy"
        @click="$emit('upload', 'project_3mf', setup.id)"
      >
        Upload 3MF</button
      ><button
        v-if="operator"
        class="btn btn-secondary"
        :disabled="busy"
        @click="$emit('upload', 'slice', setup.id)"
      >
        Upload slice</button
      ><button
        v-if="operator && !isDefault"
        class="btn btn-secondary"
        :disabled="busy"
        @click="$emit('setDefault', setup.id)"
      >
        Set default setup</button
      ><button
        v-if="admin"
        class="btn btn-secondary"
        :disabled="busy"
        @click="$emit('deleteSetup', setup)"
      >
        Delete setup
      </button>
    </div>
    <form
      v-if="operator"
      @submit.prevent="$emit('setProfile', setup.id, profile)"
    >
      <label
        >Default profile <input v-model="profile" :disabled="busy"
      /></label>
      <button class="btn btn-secondary" :disabled="busy">Save profile</button>
    </form>
    <p v-else>Default profile: {{ setup.default_profile || "—" }}</p>
    <label
      ><input v-model="showReplaced" type="checkbox" :disabled="busy" /> Show
      replaced</label
    >
    <section v-for="(rev, index) in setup.revisions" :key="rev.id">
      <h4>
        Revision {{ rev.number }} · model rev {{ rev.model_revision_number }}
      </h4>
      <p v-if="rev.note">{{ rev.note }}</p>
      <PartFiles
        :files="rev.project ? [rev.project] : []"
        :admin="admin"
        :busy="busy"
        @delete="$emit('deleteFile', $event)"
      /><button
        v-if="admin"
        class="btn btn-secondary"
        :disabled="busy"
        @click="
          $emit(
            'deleteRevision',
            rev.id,
            rev.number,
            rev.slices.length + (rev.project ? 1 : 0),
          )
        "
      >
        Delete setup revision
      </button>
      <div class="table-wrap">
        <table>
          <thead>
            <tr>
              <th>Slice</th>
              <th>Family</th>
              <th>Nozzle</th>
              <th>Profile</th>
              <th>Filament</th>
              <th>Time</th>
              <th>Uploaded</th>
              <th>Flags</th>
              <th v-if="admin">Actions</th>
            </tr>
          </thead>
          <tbody>
            <tr
              v-for="slice in rev.slices.filter(
                (s) => showReplaced || !s.superseded_by,
              )"
              :key="slice.id"
            >
              <td>
                <a :href="filesPath(slice.id, 'download')">{{ slice.name }}</a>
                <details>
                  <summary>Details</summary>
                  <PrintFacts :meta="slice.meta" :size="slice.size" />
                </details>
              </td>
              <td>{{ slice.target?.family ?? "—" }}</td>
              <td>{{ slice.target?.nozzle ?? "—" }}</td>
              <td>{{ slice.target?.profile ?? "—" }}</td>
              <td>{{ slice.meta?.filament_type ?? "—" }}</td>
              <td>{{ formatDuration(slice.meta?.estimated_print_time) }}</td>
              <td>{{ formatDate(slice.uploaded_at) }}</td>
              <td>
                <span v-if="index">from r{{ rev.number }} </span
                ><span v-if="slice.superseded_by">replaced</span>
              </td>
              <td v-if="admin">
                <button
                  class="btn btn-secondary"
                  :disabled="busy"
                  @click="$emit('deleteFile', slice)"
                >
                  Delete file
                </button>
              </td>
            </tr>
          </tbody>
        </table>
      </div>
    </section>
  </article>
</template>
<style scoped>
.card {
  border: 1px solid var(--st-border, #ccc);
  padding: var(--s4);
  margin-block: var(--s4);
}
.actions {
  display: flex;
  flex-wrap: wrap;
  gap: var(--s2);
}
</style>
