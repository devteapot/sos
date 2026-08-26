--!strict

export type Theme = {
    color: {[string]: number},
    space: { xs: number, sm: number, md: number, lg: number, xl: number },
    radius: { control: number, card: number, panel: number },
}

local theme: Theme = {
    color = {
        ink = 0x17211B,
        muted = 0x637069,
        paper = 0xF3F1E8,
        white = 0xFEFDF8,
        moss = 0x3E6B52,
        mint = 0xDCE8D8,
        amber = 0xE9A23B,
        alert = 0x8C3A36,
        rose = 0xF2DDD7,
        sand = 0xE8E4D7,
        panel = 0xECE9DC,
    },
    space = { xs = 4, sm = 8, md = 12, lg = 16, xl = 24 },
    radius = { control = 12, card = 18, panel = 22 },
}

return theme
