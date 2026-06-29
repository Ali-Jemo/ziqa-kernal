const youcanvas = @import("youcanvas");
const Rect = youcanvas.Rect;

pub const Role = enum(u8) {
    window = 0,
    panel = 1,
    label = 2,
    button = 3,
    checkbox = 4,
    slider = 5,
    text_input = 6,
};

pub const State = packed struct(u32) {
    focused: bool = false,
    disabled: bool = false,
    checked: bool = false,
    pressed: bool = false,
    _pad: u28 = 0,
};

pub const Node = struct {
    id: u64,
    parent: u64,
    role: Role,
    rect: Rect,
    state: State,
    name: []const u8,
};

pub const Tree = struct {
    nodes: []Node,
    len: usize = 0,

    pub fn init(nodes: []Node) Tree {
        return .{ .nodes = nodes };
    }

    pub fn reset(self: *Tree) void {
        self.len = 0;
    }

    pub fn push(self: *Tree, node: Node) bool {
        if (self.len >= self.nodes.len) return false;
        self.nodes[self.len] = node;
        self.len += 1;
        return true;
    }
};
