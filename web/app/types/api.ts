// Hand-mirrored from crates/shared/src/{lib,auth,printers}.rs. Timestamps are Unix seconds.
// Optional Rust fields are `field?: T | null`.

export type Role = 'viewer' | 'operator' | 'admin'

export interface SessionInfo { username: string; role: Role; csrf_token: string }
export interface WhoAmI { username: string; role: Role }
export interface UserSummary { username: string; role: Role; disabled: boolean; created_at: number }
export interface NewUser { username: string; password: string; role: Role }
export interface UpdateUser { role?: Role; disabled?: boolean }
export interface ChangePassword { current_password: string; new_password: string }
export interface SetupRequest { setup_code: string; username: string; password: string }
export interface LoginRequest { username: string; password: string }

export interface AppGrant {
  id: string
  client_id: string
  client_name: string
  username: string
  role: Role
  resource?: string | null
  created_at: number
  last_used_at?: number | null
  expires_at: number
}

export interface ConsentRequest {
  id: string
  client_id: string
  client_name: string
  client_name_self_declared: boolean
  redirect_host: string
  client_registered_at: number
  requested_role: Role
  user_role: Role
  default_role: Role
  max_role: Role
}

export interface ErrorBody { error: string }

export type PrinterKind = 'mk4' | 'mini'
export interface PrinterInfo {
  name: string
  kind: PrinterKind
  firmware: string
  api_version: string
  hostname?: string | null
  serial?: string | null
  nozzle_diameter?: number | null
  mmu?: boolean | null
}
export interface PrinterSummary { name: string; kind: PrinterKind; host: string; username: string }
export interface PrinterStatus {
  name: string
  state: string
  nozzle_temp?: number | null
  nozzle_target?: number | null
  bed_temp?: number | null
  bed_target?: number | null
}
export interface StorageDevice {
  name?: string | null
  kind: string
  path: string
  available: boolean
  read_only?: boolean | null
  free_space?: number | null
  total_space?: number | null
}

export interface PrintMeta {
  estimated_print_time?: number | null
  filament_type?: string | null
  layer_height?: number | null
  nozzle_diameter?: number | null
  printer_model?: string | null
  nozzle_temp?: number | null
  bed_temp?: number | null
  filament_used_g?: number | null
  filament_used_mm?: number | null
}

export interface JobFile {
  name: string
  display_name?: string | null
  path: string
  display_path?: string | null
  size?: number | null
  modified?: number | null
  meta?: PrintMeta | null
}

export interface Job {
  id: number
  state: string
  /** Percentage, 0–100. */
  progress: number
  time_remaining?: number | null
  time_printing: number
  inaccurate_estimates?: boolean | null
  serial_print: boolean
  file?: JobFile | null
}

export interface Transfer {
  id?: number | null
  kind: string
  display_name: string
  path: string
  url?: string | null
  size?: number | null
  progress: number
  transferred: number
  time_remaining?: number | null
  time_transferring: number
  to_print: boolean
}

export type FileKind = 'print_file' | 'firmware' | 'file' | 'folder' | 'other'

export interface FileEntry {
  name: string
  display_name?: string | null
  kind: FileKind
  read_only: boolean
  size?: number | null
  modified?: number | null
  meta?: PrintMeta | null
  has_thumbnail?: boolean
  children?: FileEntry[] | null
}

export interface Camera {
  id: string
  name?: string | null
  driver?: string | null
  path?: string | null
  resolution?: string | null
  connected: boolean
  detected: boolean
  stored: boolean
  linked: boolean
}
