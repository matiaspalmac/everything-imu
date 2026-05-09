/* Hot-string translations. Full UI keeps English as the source-of-truth;
 * this file translates only labels visible in the always-on chrome:
 * sidebar navigation, status bar, command palette, page titles, key
 * confirmations. Pages with dense English copy (Help, About) stay
 * untranslated until a contributor lands a full pass. */
export const resources = {
  en: {
    translation: {
      nav: {
        dashboard: "Dashboard",
        connection: "Connection",
        devices: "Devices",
        logs: "Logs",
        settings: "Settings",
        help: "Help",
      },
      status: {
        live: "live",
        stalled: "stalled",
        idle: "idle",
        trackers_one: "{{count}} tracker",
        trackers_other: "{{count}} trackers",
      },
      pages: {
        connection: "Connection",
        devices: "Devices",
        logs: "Logs",
        settings: "Settings",
        broadcast_actions: "Broadcast actions",
        live_trackers: "Live trackers",
        per_tracker_rate: "Per-tracker rate",
        activity: "Activity",
        orientation: "Orientation",
        reset_actions: "Reset actions",
        rotation_offset: "Rotation offset",
        per_device_config: "Per-device configuration",
      },
      actions: {
        yaw_reset: "Yaw Reset",
        full_reset: "Full Reset",
        mounting_calibrate: "Mounting Calibrate",
        save_changes: "Save changes",
        apply: "apply",
        back: "back",
      },
      cards: {
        slime_connection: "SlimeVR-Server connection",
        diagnostics: "Diagnostics",
        appearance: "Appearance",
        startup: "Startup",
        tips: "Tips",
        developer: "Developer",
        about: "About",
      },
      labels: {
        server_address: "Server address",
        log_level: "Log level",
        launch_on_startup: "Launch on system startup",
        command_palette: "Command palette",
      },
    },
  },
  es: {
    translation: {
      nav: {
        dashboard: "Panel",
        connection: "Conexión",
        devices: "Dispositivos",
        logs: "Registros",
        settings: "Ajustes",
        help: "Ayuda",
      },
      status: {
        live: "en vivo",
        stalled: "estancado",
        idle: "inactivo",
        trackers_one: "{{count}} tracker",
        trackers_other: "{{count}} trackers",
      },
      pages: {
        connection: "Conexión",
        devices: "Dispositivos",
        logs: "Registros",
        settings: "Ajustes",
        broadcast_actions: "Acciones globales",
        live_trackers: "Trackers en vivo",
        per_tracker_rate: "Tasa por tracker",
        activity: "Actividad",
        orientation: "Orientación",
        reset_actions: "Reset",
        rotation_offset: "Offset de rotación",
        per_device_config: "Configuración por dispositivo",
      },
      actions: {
        yaw_reset: "Reset Yaw",
        full_reset: "Reset Completo",
        mounting_calibrate: "Calibrar montaje",
        save_changes: "Guardar cambios",
        apply: "aplicar",
        back: "volver",
      },
      cards: {
        slime_connection: "Conexión a SlimeVR-Server",
        diagnostics: "Diagnóstico",
        appearance: "Apariencia",
        startup: "Inicio",
        tips: "Atajos",
        developer: "Desarrollo",
        about: "Acerca de",
      },
      labels: {
        server_address: "Dirección del servidor",
        log_level: "Nivel de log",
        launch_on_startup: "Lanzar al iniciar el sistema",
        command_palette: "Paleta de comandos",
      },
    },
  },
} as const;

export type SupportedLocale = keyof typeof resources;
