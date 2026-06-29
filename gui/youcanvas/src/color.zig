pub const Color = packed struct(u32) {
    value: u32,

    pub const black = Color{ .value = 0xFF000000 };
    pub const white = Color{ .value = 0xFFFFFFFF };

    pub fn rgba(r: u8, g: u8, b: u8, a: u8) Color {
        return .{ .value = (@as(u32, a) << 24) | (@as(u32, r) << 16) | (@as(u32, g) << 8) | @as(u32, b) };
    }

    pub fn xrgb(r: u8, g: u8, b: u8) Color {
        return rgba(r, g, b, 0xFF);
    }

    pub fn to_u32(self: Color) u32 {
        return self.value;
    }

    pub fn to_xrgb(self: Color) u32 {
        return self.value | 0xFF000000;
    }
};

pub const Sumerian = struct {
    pub const near_black: u32 = 0x000A0A0A;
    pub const bg_card: u32 = 0x00161616;
    pub const bg_elevated: u32 = 0x001E1E1E;
    pub const bg_hover: u32 = 0x002A2620;
    pub const gold: u32 = 0x00C8A951;
    pub const gold_dim: u32 = 0x008B7340;
    pub const gold_glow: u32 = 0x00DFBE6A;
    pub const text_hi: u32 = 0x00F0E8CC;
    pub const text_lo: u32 = 0x009B9080;
    pub const border: u32 = 0x002A2620;
    pub const success: u32 = 0x0010B981;
    pub const err: u32 = 0x00EF4444;
    pub const info: u32 = 0x003B82F6;
};

pub const Theme = struct {
    bg: u32 = Sumerian.bg_card,
    panel: u32 = Sumerian.bg_elevated,
    hover: u32 = Sumerian.bg_hover,
    text: u32 = Sumerian.text_hi,
    muted: u32 = Sumerian.text_lo,
    accent: u32 = Sumerian.gold,
    border: u32 = Sumerian.border,

    pub fn default() Theme {
        return .{};
    }
};
