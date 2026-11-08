use crate::error::ThkResult;
use crate::paths::SafePath;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::Path;

pub const MOBILE_CONTROLS_FILE: &str = "mobile_controls.json";

pub const GLFW_MOUSE_BUTTON_LEFT: i32 = 0;
pub const GLFW_MOUSE_BUTTON_RIGHT: i32 = 1;
pub const GLFW_MOUSE_BUTTON_MIDDLE: i32 = 2;

pub const GLFW_KEY_SPACE: i32 = 32;
pub const GLFW_KEY_SLASH: i32 = 47;
pub const GLFW_KEY_0: i32 = 48;
pub const GLFW_KEY_1: i32 = 49;
pub const GLFW_KEY_2: i32 = 50;
pub const GLFW_KEY_3: i32 = 51;
pub const GLFW_KEY_4: i32 = 52;
pub const GLFW_KEY_5: i32 = 53;
pub const GLFW_KEY_6: i32 = 54;
pub const GLFW_KEY_7: i32 = 55;
pub const GLFW_KEY_8: i32 = 56;
pub const GLFW_KEY_9: i32 = 57;

pub const GLFW_KEY_A: i32 = 65;
pub const GLFW_KEY_B: i32 = 66;
pub const GLFW_KEY_C: i32 = 67;
pub const GLFW_KEY_D: i32 = 68;
pub const GLFW_KEY_E: i32 = 69;
pub const GLFW_KEY_F: i32 = 70;
pub const GLFW_KEY_G: i32 = 71;
pub const GLFW_KEY_H: i32 = 72;
pub const GLFW_KEY_I: i32 = 73;
pub const GLFW_KEY_J: i32 = 74;
pub const GLFW_KEY_K: i32 = 75;
pub const GLFW_KEY_L: i32 = 76;
pub const GLFW_KEY_M: i32 = 77;
pub const GLFW_KEY_N: i32 = 78;
pub const GLFW_KEY_O: i32 = 79;
pub const GLFW_KEY_P: i32 = 80;
pub const GLFW_KEY_Q: i32 = 81;
pub const GLFW_KEY_R: i32 = 82;
pub const GLFW_KEY_S: i32 = 83;
pub const GLFW_KEY_T: i32 = 84;
pub const GLFW_KEY_U: i32 = 85;
pub const GLFW_KEY_V: i32 = 86;
pub const GLFW_KEY_W: i32 = 87;
pub const GLFW_KEY_X: i32 = 88;
pub const GLFW_KEY_Y: i32 = 89;
pub const GLFW_KEY_Z: i32 = 90;

pub const GLFW_KEY_ESCAPE: i32 = 256;
pub const GLFW_KEY_ENTER: i32 = 257;
pub const GLFW_KEY_TAB: i32 = 258;
pub const GLFW_KEY_BACKSPACE: i32 = 259;
pub const GLFW_KEY_RIGHT: i32 = 262;
pub const GLFW_KEY_LEFT: i32 = 263;
pub const GLFW_KEY_DOWN: i32 = 264;
pub const GLFW_KEY_UP: i32 = 265;

pub const GLFW_KEY_F1: i32 = 290;
pub const GLFW_KEY_F2: i32 = 291;
pub const GLFW_KEY_F3: i32 = 292;
pub const GLFW_KEY_F4: i32 = 293;
pub const GLFW_KEY_F5: i32 = 294;
pub const GLFW_KEY_F6: i32 = 295;
pub const GLFW_KEY_F7: i32 = 296;
pub const GLFW_KEY_F8: i32 = 297;
pub const GLFW_KEY_F9: i32 = 298;
pub const GLFW_KEY_F10: i32 = 299;
pub const GLFW_KEY_F11: i32 = 300;
pub const GLFW_KEY_F12: i32 = 301;

pub const GLFW_KEY_LEFT_SHIFT: i32 = 340;
pub const GLFW_KEY_LEFT_CONTROL: i32 = 341;
pub const GLFW_KEY_LEFT_ALT: i32 = 342;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ControlShape {
    Dpad,
    Round,
    Rect,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ControlBinding {
    Key {
        primary: i32,
        #[serde(default)]
        secondary: i32,
    },
    MouseButton {
        button: i32,
    },
    ScrollUp,
    ScrollDown,
    OpenChat,
    ToggleVirtualMouse,
}

impl ControlBinding {
    pub fn display_label(&self) -> &'static str {
        for opt in bindable_catalog() {
            if opt.binding == *self {
                return opt.label;
            }
        }
        match self {
            Self::Key { .. } => "Phím tùy chọn",
            Self::MouseButton { button: 0 } => "Chuột Trái (Đánh)",
            Self::MouseButton { button: 1 } => "Chuột Phải (Đặt/Dùng)",
            Self::MouseButton { .. } => "Chuột Giữa (Chọn khối)",
            Self::ScrollUp => "Cuộn Lên",
            Self::ScrollDown => "Cuộn Xuống",
            Self::OpenChat => "Mở Chat / Lệnh",
            Self::ToggleVirtualMouse => "Bật / Tắt Chuột Ảo",
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct BindableKeyOption {
    pub label: &'static str,
    pub short_name: &'static str,
    pub binding: ControlBinding,
}

const BINDABLE_CATALOG: &[BindableKeyOption] = &[
    BindableKeyOption {
        label: "Chuột Trái (Đánh / Phá)",
        short_name: "Đánh",
        binding: ControlBinding::MouseButton {
            button: GLFW_MOUSE_BUTTON_LEFT,
        },
    },
    BindableKeyOption {
        label: "Chuột Phải (Đặt / Dùng)",
        short_name: "Đặt",
        binding: ControlBinding::MouseButton {
            button: GLFW_MOUSE_BUTTON_RIGHT,
        },
    },
    BindableKeyOption {
        label: "Chuột Giữa (Chọn khối)",
        short_name: "Giữa",
        binding: ControlBinding::MouseButton {
            button: GLFW_MOUSE_BUTTON_MIDDLE,
        },
    },
    BindableKeyOption {
        label: "Cuộn Chuột Lên",
        short_name: "Cuộn+",
        binding: ControlBinding::ScrollUp,
    },
    BindableKeyOption {
        label: "Cuộn Chuột Xuống",
        short_name: "Cuộn-",
        binding: ControlBinding::ScrollDown,
    },
    BindableKeyOption {
        label: "Mở Khung Chat / Lệnh",
        short_name: "Chat",
        binding: ControlBinding::OpenChat,
    },
    BindableKeyOption {
        label: "Bật / Tắt Chuột Ảo",
        short_name: "Chuột",
        binding: ControlBinding::ToggleVirtualMouse,
    },
    BindableKeyOption {
        label: "Space (Nhảy / Bay)",
        short_name: "Nhảy",
        binding: ControlBinding::Key {
            primary: GLFW_KEY_SPACE,
            secondary: 0,
        },
    },
    BindableKeyOption {
        label: "Left Shift (Ngồi / Lén)",
        short_name: "Ngồi",
        binding: ControlBinding::Key {
            primary: GLFW_KEY_LEFT_SHIFT,
            secondary: 0,
        },
    },
    BindableKeyOption {
        label: "Left Ctrl (Chạy nhanh)",
        short_name: "Chạy",
        binding: ControlBinding::Key {
            primary: GLFW_KEY_LEFT_CONTROL,
            secondary: 0,
        },
    },
    BindableKeyOption {
        label: "E (Túi đồ)",
        short_name: "Túi E",
        binding: ControlBinding::Key {
            primary: GLFW_KEY_E,
            secondary: 0,
        },
    },
    BindableKeyOption {
        label: "Q (Vứt vật phẩm)",
        short_name: "Vứt",
        binding: ControlBinding::Key {
            primary: GLFW_KEY_Q,
            secondary: 0,
        },
    },
    BindableKeyOption {
        label: "F (Đổi tay phụ)",
        short_name: "Tay phụ",
        binding: ControlBinding::Key {
            primary: GLFW_KEY_F,
            secondary: 0,
        },
    },
    BindableKeyOption {
        label: "ESC (Tạm dừng / Đóng)",
        short_name: "ESC",
        binding: ControlBinding::Key {
            primary: GLFW_KEY_ESCAPE,
            secondary: 0,
        },
    },
    BindableKeyOption {
        label: "Tab (Danh sách người chơi)",
        short_name: "Tab",
        binding: ControlBinding::Key {
            primary: GLFW_KEY_TAB,
            secondary: 0,
        },
    },
    BindableKeyOption {
        label: "Enter (Xác nhận)",
        short_name: "Enter",
        binding: ControlBinding::Key {
            primary: GLFW_KEY_ENTER,
            secondary: 0,
        },
    },
    BindableKeyOption {
        label: "F1 (Ẩn HUD trong game)",
        short_name: "F1",
        binding: ControlBinding::Key {
            primary: GLFW_KEY_F1,
            secondary: 0,
        },
    },
    BindableKeyOption {
        label: "F2 (Chụp ảnh trong game)",
        short_name: "F2",
        binding: ControlBinding::Key {
            primary: GLFW_KEY_F2,
            secondary: 0,
        },
    },
    BindableKeyOption {
        label: "F3 (Thông tin tọa độ)",
        short_name: "F3",
        binding: ControlBinding::Key {
            primary: GLFW_KEY_F3,
            secondary: 0,
        },
    },
    BindableKeyOption {
        label: "F3 + A (Tải lại Chunk)",
        short_name: "F3+A",
        binding: ControlBinding::Key {
            primary: GLFW_KEY_F3,
            secondary: GLFW_KEY_A,
        },
    },
    BindableKeyOption {
        label: "F3 + B (Hiện Hitbox)",
        short_name: "F3+B",
        binding: ControlBinding::Key {
            primary: GLFW_KEY_F3,
            secondary: GLFW_KEY_B,
        },
    },
    BindableKeyOption {
        label: "F3 + G (Ranh giới Chunk)",
        short_name: "F3+G",
        binding: ControlBinding::Key {
            primary: GLFW_KEY_F3,
            secondary: GLFW_KEY_G,
        },
    },
    BindableKeyOption {
        label: "F5 (Đổi góc nhìn)",
        short_name: "F5",
        binding: ControlBinding::Key {
            primary: GLFW_KEY_F5,
            secondary: 0,
        },
    },
    BindableKeyOption {
        label: "Phím 1 (Ô nhanh 1)",
        short_name: "1",
        binding: ControlBinding::Key {
            primary: GLFW_KEY_1,
            secondary: 0,
        },
    },
    BindableKeyOption {
        label: "Phím 2 (Ô nhanh 2)",
        short_name: "2",
        binding: ControlBinding::Key {
            primary: GLFW_KEY_2,
            secondary: 0,
        },
    },
    BindableKeyOption {
        label: "Phím 3 (Ô nhanh 3)",
        short_name: "3",
        binding: ControlBinding::Key {
            primary: GLFW_KEY_3,
            secondary: 0,
        },
    },
    BindableKeyOption {
        label: "Phím 4 (Ô nhanh 4)",
        short_name: "4",
        binding: ControlBinding::Key {
            primary: GLFW_KEY_4,
            secondary: 0,
        },
    },
    BindableKeyOption {
        label: "Phím 5 (Ô nhanh 5)",
        short_name: "5",
        binding: ControlBinding::Key {
            primary: GLFW_KEY_5,
            secondary: 0,
        },
    },
    BindableKeyOption {
        label: "Phím 6 (Ô nhanh 6)",
        short_name: "6",
        binding: ControlBinding::Key {
            primary: GLFW_KEY_6,
            secondary: 0,
        },
    },
    BindableKeyOption {
        label: "Phím 7 (Ô nhanh 7)",
        short_name: "7",
        binding: ControlBinding::Key {
            primary: GLFW_KEY_7,
            secondary: 0,
        },
    },
    BindableKeyOption {
        label: "Phím 8 (Ô nhanh 8)",
        short_name: "8",
        binding: ControlBinding::Key {
            primary: GLFW_KEY_8,
            secondary: 0,
        },
    },
    BindableKeyOption {
        label: "Phím 9 (Ô nhanh 9)",
        short_name: "9",
        binding: ControlBinding::Key {
            primary: GLFW_KEY_9,
            secondary: 0,
        },
    },
    BindableKeyOption {
        label: "Phím C (Zoom / Mod)",
        short_name: "C",
        binding: ControlBinding::Key {
            primary: GLFW_KEY_C,
            secondary: 0,
        },
    },
    BindableKeyOption {
        label: "Phím G (Mod / Kỹ năng)",
        short_name: "G",
        binding: ControlBinding::Key {
            primary: GLFW_KEY_G,
            secondary: 0,
        },
    },
    BindableKeyOption {
        label: "Phím H (Mod)",
        short_name: "H",
        binding: ControlBinding::Key {
            primary: GLFW_KEY_H,
            secondary: 0,
        },
    },
    BindableKeyOption {
        label: "Phím J (Bản đồ lớn)",
        short_name: "J",
        binding: ControlBinding::Key {
            primary: GLFW_KEY_J,
            secondary: 0,
        },
    },
    BindableKeyOption {
        label: "Phím K (Shader / Mod)",
        short_name: "K",
        binding: ControlBinding::Key {
            primary: GLFW_KEY_K,
            secondary: 0,
        },
    },
    BindableKeyOption {
        label: "Phím L (Thành tựu)",
        short_name: "L",
        binding: ControlBinding::Key {
            primary: GLFW_KEY_L,
            secondary: 0,
        },
    },
    BindableKeyOption {
        label: "Phím M (Minimap)",
        short_name: "M",
        binding: ControlBinding::Key {
            primary: GLFW_KEY_M,
            secondary: 0,
        },
    },
    BindableKeyOption {
        label: "Phím O (Ẩn giao diện Mod)",
        short_name: "O",
        binding: ControlBinding::Key {
            primary: GLFW_KEY_O,
            secondary: 0,
        },
    },
    BindableKeyOption {
        label: "Phím R (Công thức / Nạp)",
        short_name: "R",
        binding: ControlBinding::Key {
            primary: GLFW_KEY_R,
            secondary: 0,
        },
    },
    BindableKeyOption {
        label: "Phím U (Công dụng vật phẩm)",
        short_name: "U",
        binding: ControlBinding::Key {
            primary: GLFW_KEY_U,
            secondary: 0,
        },
    },
    BindableKeyOption {
        label: "Phím V (Voice Chat / Mod)",
        short_name: "V",
        binding: ControlBinding::Key {
            primary: GLFW_KEY_V,
            secondary: 0,
        },
    },
    BindableKeyOption {
        label: "Phím X (Mod)",
        short_name: "X",
        binding: ControlBinding::Key {
            primary: GLFW_KEY_X,
            secondary: 0,
        },
    },
    BindableKeyOption {
        label: "Phím Z (Zoom / Nằm)",
        short_name: "Z",
        binding: ControlBinding::Key {
            primary: GLFW_KEY_Z,
            secondary: 0,
        },
    },
    BindableKeyOption {
        label: "Left Alt (Xem tự do / Mod)",
        short_name: "Alt",
        binding: ControlBinding::Key {
            primary: GLFW_KEY_LEFT_ALT,
            secondary: 0,
        },
    },
];

pub fn bindable_catalog() -> &'static [BindableKeyOption] {
    BINDABLE_CATALOG
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MobileControlButton {
    pub id: String,
    pub label: String,
    pub x_norm: f32,
    pub y_norm: f32,
    pub width_dp: f32,
    pub height_dp: f32,
    pub scale: f32,
    pub opacity: f32,
    #[serde(default)]
    pub toggle_mode: bool,
    pub shape: ControlShape,
    pub binding: ControlBinding,
}

impl MobileControlButton {
    pub fn sanitize(&mut self) {
        if !self.x_norm.is_finite() {
            self.x_norm = 0.5;
        }
        if !self.y_norm.is_finite() {
            self.y_norm = 0.5;
        }
        if !self.width_dp.is_finite() {
            self.width_dp = 48.0;
        }
        if !self.height_dp.is_finite() {
            self.height_dp = 48.0;
        }
        if !self.scale.is_finite() {
            self.scale = 1.0;
        }
        if !self.opacity.is_finite() {
            self.opacity = 0.78;
        }
        self.x_norm = self.x_norm.clamp(0.03, 0.97);
        self.y_norm = self.y_norm.clamp(0.04, 0.96);
        self.width_dp = self.width_dp.clamp(28.0, 240.0);
        self.height_dp = self.height_dp.clamp(28.0, 240.0);
        self.scale = self.scale.clamp(0.5, 2.2);
        self.opacity = self.opacity.clamp(0.15, 1.0);
        if self.label.trim().is_empty() && self.shape != ControlShape::Dpad {
            self.label = "Phím".to_string();
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MobileControlLayout {
    pub version: u32,
    pub mouse_sensitivity: f32,
    pub buttons: Vec<MobileControlButton>,
}

impl Default for MobileControlLayout {
    fn default() -> Self {
        Self::bedrock_default()
    }
}

impl MobileControlLayout {
    pub fn bedrock_default() -> Self {
        let mut buttons = Vec::with_capacity(24);

        buttons.push(MobileControlButton {
            id: "bedrock_dpad".into(),
            label: "Di chuyển".into(),
            x_norm: 0.14,
            y_norm: 0.71,
            width_dp: 156.0,
            height_dp: 156.0,
            scale: 1.0,
            opacity: 0.80,
            toggle_mode: false,
            shape: ControlShape::Dpad,
            binding: ControlBinding::Key {
                primary: GLFW_KEY_W,
                secondary: 0,
            },
        });

        buttons.push(MobileControlButton {
            id: "sprint".into(),
            label: "Chạy".into(),
            x_norm: 0.27,
            y_norm: 0.56,
            width_dp: 48.0,
            height_dp: 48.0,
            scale: 1.0,
            opacity: 0.78,
            toggle_mode: true,
            shape: ControlShape::Round,
            binding: ControlBinding::Key {
                primary: GLFW_KEY_LEFT_CONTROL,
                secondary: 0,
            },
        });

        buttons.push(MobileControlButton {
            id: "jump".into(),
            label: "Nhảy".into(),
            x_norm: 0.90,
            y_norm: 0.72,
            width_dp: 66.0,
            height_dp: 66.0,
            scale: 1.0,
            opacity: 0.82,
            toggle_mode: false,
            shape: ControlShape::Round,
            binding: ControlBinding::Key {
                primary: GLFW_KEY_SPACE,
                secondary: 0,
            },
        });

        buttons.push(MobileControlButton {
            id: "attack".into(),
            label: "Đánh".into(),
            x_norm: 0.79,
            y_norm: 0.77,
            width_dp: 58.0,
            height_dp: 58.0,
            scale: 1.0,
            opacity: 0.82,
            toggle_mode: false,
            shape: ControlShape::Round,
            binding: ControlBinding::MouseButton {
                button: GLFW_MOUSE_BUTTON_LEFT,
            },
        });

        buttons.push(MobileControlButton {
            id: "use_item".into(),
            label: "Đặt".into(),
            x_norm: 0.84,
            y_norm: 0.55,
            width_dp: 58.0,
            height_dp: 58.0,
            scale: 1.0,
            opacity: 0.82,
            toggle_mode: false,
            shape: ControlShape::Round,
            binding: ControlBinding::MouseButton {
                button: GLFW_MOUSE_BUTTON_RIGHT,
            },
        });

        buttons.push(MobileControlButton {
            id: "drop".into(),
            label: "Vứt".into(),
            x_norm: 0.74,
            y_norm: 0.58,
            width_dp: 46.0,
            height_dp: 46.0,
            scale: 1.0,
            opacity: 0.75,
            toggle_mode: false,
            shape: ControlShape::Round,
            binding: ControlBinding::Key {
                primary: GLFW_KEY_Q,
                secondary: 0,
            },
        });

        buttons.push(MobileControlButton {
            id: "offhand".into(),
            label: "Tay phụ".into(),
            x_norm: 0.92,
            y_norm: 0.49,
            width_dp: 48.0,
            height_dp: 46.0,
            scale: 1.0,
            opacity: 0.75,
            toggle_mode: false,
            shape: ControlShape::Round,
            binding: ControlBinding::Key {
                primary: GLFW_KEY_F,
                secondary: 0,
            },
        });

        let hotbar_start_x = 0.29_f32;
        let hotbar_step_x = 0.046_f32;
        for idx in 0..9_i32 {
            let slot_num = idx + 1;
            buttons.push(MobileControlButton {
                id: format!("hotbar_{slot_num}"),
                label: slot_num.to_string(),
                x_norm: hotbar_start_x + (idx as f32) * hotbar_step_x,
                y_norm: 0.93,
                width_dp: 36.0,
                height_dp: 36.0,
                scale: 1.0,
                opacity: 0.76,
                toggle_mode: false,
                shape: ControlShape::Rect,
                binding: ControlBinding::Key {
                    primary: GLFW_KEY_1 + idx,
                    secondary: 0,
                },
            });
        }

        buttons.push(MobileControlButton {
            id: "inventory".into(),
            label: "•••".into(),
            x_norm: hotbar_start_x + 9.0 * hotbar_step_x + 0.01,
            y_norm: 0.93,
            width_dp: 44.0,
            height_dp: 36.0,
            scale: 1.0,
            opacity: 0.82,
            toggle_mode: false,
            shape: ControlShape::Rect,
            binding: ControlBinding::Key {
                primary: GLFW_KEY_E,
                secondary: 0,
            },
        });

        let top_items: &[(&str, &str, f32, ControlBinding)] = &[
            (
                "pause_esc",
                "ESC",
                0.28,
                ControlBinding::Key {
                    primary: GLFW_KEY_ESCAPE,
                    secondary: 0,
                },
            ),
            ("open_chat", "Chat", 0.36, ControlBinding::OpenChat),
            (
                "camera_f5",
                "F5",
                0.44,
                ControlBinding::Key {
                    primary: GLFW_KEY_F5,
                    secondary: 0,
                },
            ),
            (
                "debug_f3",
                "F3",
                0.51,
                ControlBinding::Key {
                    primary: GLFW_KEY_F3,
                    secondary: 0,
                },
            ),
            (
                "player_tab",
                "Tab",
                0.58,
                ControlBinding::Key {
                    primary: GLFW_KEY_TAB,
                    secondary: 0,
                },
            ),
            (
                "virtual_mouse",
                "Chuột",
                0.67,
                ControlBinding::ToggleVirtualMouse,
            ),
        ];

        for (id, label, x_norm, binding) in top_items {
            buttons.push(MobileControlButton {
                id: (*id).into(),
                label: (*label).into(),
                x_norm: *x_norm,
                y_norm: 0.065,
                width_dp: 46.0,
                height_dp: 30.0,
                scale: 1.0,
                opacity: 0.76,
                toggle_mode: false,
                shape: ControlShape::Rect,
                binding: *binding,
            });
        }

        Self {
            version: 1,
            mouse_sensitivity: 1.35,
            buttons,
        }
    }

    pub fn sanitize(&mut self) {
        if !self.mouse_sensitivity.is_finite() {
            self.mouse_sensitivity = 1.35;
        }
        self.mouse_sensitivity = self.mouse_sensitivity.clamp(0.25, 4.0);
        for btn in &mut self.buttons {
            btn.sanitize();
        }
        if self.buttons.is_empty() {
            *self = Self::bedrock_default();
        }
    }

    pub fn load_or_default(root: &Path) -> Self {
        let Ok(file_path) = SafePath::resolve_child(root, MOBILE_CONTROLS_FILE) else {
            return Self::bedrock_default();
        };
        let Ok(raw) = fs::read_to_string(&file_path) else {
            return Self::bedrock_default();
        };
        let Ok(mut parsed) = serde_json::from_str::<Self>(&raw) else {
            return Self::bedrock_default();
        };
        parsed.sanitize();
        parsed
    }

    pub fn save(&self, root: &Path) -> ThkResult<()> {
        fs::create_dir_all(root)?;
        let mut clean = self.clone();
        clean.sanitize();
        let serialized = serde_json::to_string_pretty(&clean)?;
        let target = SafePath::resolve_child(root, MOBILE_CONTROLS_FILE)?;
        let tmp = SafePath::resolve_child(root, format!("{MOBILE_CONTROLS_FILE}.tmp"))?;
        fs::write(&tmp, serialized.as_bytes())?;
        fs::rename(&tmp, &target)?;
        Ok(())
    }
}
