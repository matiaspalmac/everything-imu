import { commands, events } from "./bindings";

export const api = commands;

// MAC address bytes as the Tauri commands expect them — a fixed-length
// tuple. Re-exported here so call sites don't have to import the
// auto-generated binding shape directly.
export type Mac = [number, number, number, number, number, number];

export type {
  BiasEntry,
  BiasUpdate,
  ConnectionStatusUpdate,
  DeviceDiscovered,
  DeviceHistoryDto,
  DeviceMetadataDto,
  DeviceStateChanged,
  FusionAlgoDto,
  ImuSampleEntry,
  ImuSampleUpdate,
  IpcError,
  LatencyEntry,
  LatencyUpdate,
  LogEntry,
  LogEntryDto,
  MountingOrientationDto,
  PerDeviceSettingsDto,
  ResetKindDto,
  SettingsDto,
  TrackerSnapshot,
  TrackerUpdate,
} from "./bindings";
export { events };
