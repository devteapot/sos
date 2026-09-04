--!strict

export type Theme = {
    color: {[string]: number},
    space: { xs: number, sm: number, md: number, lg: number, xl: number },
    radius: { control: number, card: number, panel: number },
}

local theme: Theme = {
    color = {
        ink = 0xEDEDED,
        muted = 0x8B8B8B,
        paper = 0x000000,
        white = 0x0A0A0A,
        moss = 0xFFFFFF,
        mint = 0x181818,
        amber = 0xF5A524,
        alert = 0xEF4444,
        rose = 0x2A0F12,
        sand = 0x111111,
        panel = 0x1B1B1B,
        outline = 0x2A2A2A,
    },
    space = { xs = 4, sm = 8, md = 12, lg = 16, xl = 24 },
    radius = { control = 6, card = 8, panel = 10 },
}

return theme
