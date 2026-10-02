//! Line icons as SVG path data on a 24×24 viewbox, stroked (round caps and
//! joins) by each UI. Shared so both twins use the same iconography.

pub const DIVIDE: &str =
    "M5 12h14M12 5.6a1 1 0 1 1 0 2 1 1 0 1 1 0-2zM12 16.4a1 1 0 1 1 0 2 1 1 0 1 1 0-2z";
pub const MULTIPLY: &str = "M7 7l10 10M17 7L7 17";
pub const SUBTRACT: &str = "M5.5 12h13";
pub const ADD: &str = "M12 5.5v13M5.5 12h13";
pub const EQUALS: &str = "M5.5 9h13M5.5 15h13";
pub const BACKSPACE: &str =
    "M9.2 5.5h9.3a2 2 0 0 1 2 2v9a2 2 0 0 1-2 2H9.2L3.5 12zM11.5 9.5l5 5M16.5 9.5l-5 5";
pub const MENU: &str = "M4 7h16M4 12h16M4 17h16";
pub const HISTORY: &str = "M3.5 12a8.5 8.5 0 1 0 2.6-6.1M3.5 4.5v4h4M12 7.5v5l3.2 2";
pub const KEEP_ON_TOP: &str = "M3 6.5a2.5 2.5 0 0 1 2.5-2.5h13a2.5 2.5 0 0 1 2.5 2.5v11a2.5 2.5 0 0 1-2.5 2.5h-13A2.5 2.5 0 0 1 3 17.5zM12.5 12h5.5v5h-5.5z";
pub const BACK_TO_FULL: &str = "M3 6.5a2.5 2.5 0 0 1 2.5-2.5h13a2.5 2.5 0 0 1 2.5 2.5v11a2.5 2.5 0 0 1-2.5 2.5h-13A2.5 2.5 0 0 1 3 17.5zM8 15l8-6M11 9h5v5";
pub const STANDARD: &str = "M6.5 3h11A2.5 2.5 0 0 1 20 5.5v13a2.5 2.5 0 0 1-2.5 2.5h-11A2.5 2.5 0 0 1 4 18.5v-13A2.5 2.5 0 0 1 6.5 3zM7.5 6.5h9v3h-9zM8 13.5h.01M12 13.5h.01M16 13.5h.01M8 17.5h.01M12 17.5h.01M16 17.5h.01";
pub const SCIENTIFIC: &str =
    "M9 3h6M10 3v6.2L5.2 17.7A2.2 2.2 0 0 0 7.1 21h9.8a2.2 2.2 0 0 0 1.9-3.3L14 9.2V3M7.6 14.5h8.8";
pub const GRAPHING: &str = "M4 3.5v16.5h16.5M7 15.5c2.5-8 4.8-8.6 6.8-3.2 1.7 4.6 3.8 4.4 6.2-2.3";
pub const PROGRAMMER: &str = "M8 7l-5 5 5 5M16 7l5 5-5 5M13.6 4.5l-3.2 15";
pub const DATE: &str = "M5.5 5h13A1.5 1.5 0 0 1 20 6.5v12a1.5 1.5 0 0 1-1.5 1.5h-13A1.5 1.5 0 0 1 4 18.5v-12A1.5 1.5 0 0 1 5.5 5zM4 10h16M8.5 3v4M15.5 3v4M8 14h.01M12 14h.01M16 14h.01M8 17h.01M12 17h.01";
pub const CURRENCY: &str = "M12 3a9 9 0 1 0 0 18 9 9 0 0 0 0-18zM15 9.4c-.5-1-1.6-1.6-3-1.6-1.8 0-3 .9-3 2.1s1.2 1.7 3 2.1 3 .9 3 2.1-1.2 2.1-3 2.1c-1.4 0-2.5-.6-3-1.6M12 6.2v1.6M12 16.2v1.6";
pub const VOLUME: &str = "M12 3l8 4.5v9L12 21l-8-4.5v-9zM4 7.5l8 4.5 8-4.5M12 12v9";
pub const LENGTH: &str =
    "M2.8 15.6L15.6 2.8l5.6 5.6L8.4 21.2zM6.6 11.8l1.8 1.8M9.4 9l2.6 2.6M12.2 6.2l1.8 1.8";
pub const WEIGHT: &str = "M9 8.5a3 3 0 1 1 6 0M6.3 8.5h11.4l2.3 11.5H4z";
pub const TEMPERATURE: &str = "M10 4.5a2 2 0 0 1 4 0v9.6a4.2 4.2 0 1 1-4 0zM12 9.5v7.5";
pub const ENERGY: &str = "M13.5 2.5L4.5 14h7l-1 7.5 9-11.5h-7z";
pub const AREA: &str = "M4 4h16v16H4zM4 9.5h5.5V4M14.5 20v-5.5H20";
pub const SPEED: &str = "M3.8 17.5a8.6 8.6 0 1 1 16.4 0M12 14l4.2-5.2M12 14h.01M7 17.5h10";
pub const TIME: &str =
    "M12 21a7.8 7.8 0 1 0 0-15.6A7.8 7.8 0 0 0 12 21zM12 9v4.2l2.6 1.8M10 2.5h4M18.4 6.1l1.6-1.6";
pub const POWER: &str = "M9 2.8v5M15 2.8v5M6 7.8h12v3.4a6 6 0 0 1-12 0zM12 17.2v4";
pub const DATA: &str = "M12 3.5c4.4 0 8 1.3 8 3s-3.6 3-8 3-8-1.3-8-3 3.6-3 8-3zM4 6.5v11c0 1.7 3.6 3 8 3s8-1.3 8-3v-11M4 12c0 1.7 3.6 3 8 3s8-1.3 8-3";
pub const PRESSURE: &str =
    "M12 3a9 9 0 1 0 0 18 9 9 0 0 0 0-18zM12 12.5l-3.8-4M8 16.5h8M12 12.5h.01";
pub const ANGLE: &str = "M3.5 20h17M3.5 20L16.5 5M10.5 20a7 7 0 0 0-2.3-5.2";
pub const SETTINGS: &str = "M12 8.8a3.2 3.2 0 1 0 0 6.4 3.2 3.2 0 0 0 0-6.4zM12 2.8v2.4M12 18.8v2.4M2.8 12h2.4M18.8 12h2.4M5.5 5.5l1.7 1.7M16.8 16.8l1.7 1.7M5.5 18.5l1.7-1.7M16.8 7.2l1.7-1.7";
pub const SWAP: &str = "M7 4v16M3.8 7.2L7 4l3.2 3.2M17 20V4M13.8 16.8L17 20l3.2-3.2";
pub const TRASH: &str = "M4 7h16M9.5 7V4.5h5V7M6 7l1 13h10l1-13M10 11v5.5M14 11v5.5";
pub const PLUS: &str = "M12 5v14M5 12h14";
pub const CLOSE: &str = "M6 6l12 12M18 6L6 18";
pub const ZOOM_IN: &str =
    "M10.5 4a6.5 6.5 0 1 0 0 13 6.5 6.5 0 0 0 0-13zM15.3 15.3L20 20M10.5 7.8v5.4M7.8 10.5h5.4";
pub const ZOOM_OUT: &str =
    "M10.5 4a6.5 6.5 0 1 0 0 13 6.5 6.5 0 0 0 0-13zM15.3 15.3L20 20M7.8 10.5h5.4";
pub const ZOOM_FIT: &str = "M4 9V4h5M15 4h5v5M20 15v5h-5M9 20H4v-5M9 12h6M12 9v6";
pub const TRACE: &str = "M12 3v4M12 17v4M3 12h4M17 12h4M12 9a3 3 0 1 0 0 6 3 3 0 0 0 0-6z";
pub const FUNCTION: &str =
    "M15.5 4.5c-2.5-.8-4 .3-4.5 2.5l-2.5 11c-.5 2.2-2 3.3-4.5 2.5M7 10h8M14 13l5 6M19 13l-5 6";
pub const COPY: &str = "M8.5 8.5h10a1.5 1.5 0 0 1 1.5 1.5v9a1.5 1.5 0 0 1-1.5 1.5h-10A1.5 1.5 0 0 1 7 19v-9a1.5 1.5 0 0 1 1.5-1.5zM16 8.5V5.5A1.5 1.5 0 0 0 14.5 4h-9A1.5 1.5 0 0 0 4 5.5v9A1.5 1.5 0 0 0 5.5 16H7";
pub const CHEVRON_DOWN: &str = "M6 9l6 6 6-6";
pub const CHEVRON_LEFT: &str = "M15 6l-6 6 6 6";
pub const SLIDERS: &str = "M4 7h9M17 7h3M4 17h3M11 17h9M15 4.5v5M9 14.5v5";
pub const KEYBOARD: &str = "M3.5 6h17a1 1 0 0 1 1 1v10a1 1 0 0 1-1 1h-17a1 1 0 0 1-1-1V7a1 1 0 0 1 1-1zM7 10h.01M11 10h.01M15 10h.01M18 10h.01M7 14h10";
pub const BITS: &str = "M5 5h3v5H5zM5 14h3v5H5zM12 5v5M16 5h3v5h-3zM12 14v5M16 14v5";
pub const SPARKLE: &str = "M12 3l1.8 5.2L19 10l-5.2 1.8L12 17l-1.8-5.2L5 10l5.2-1.8zM18.5 15.5l.8 2.2 2.2.8-2.2.8-.8 2.2-.8-2.2-2.2-.8 2.2-.8z";
