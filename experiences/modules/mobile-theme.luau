--!strict

export type MobileTheme = {
    color: {[string]: number},
    space: { xs: number, sm: number, md: number, lg: number, xl: number },
    radius: { control: number, card: number, sheet: number },
    touch: { compact: number, standard: number, prominent: number },
}

local theme: MobileTheme = {
    color = {
        canvas = 0x101713,
        surface = 0x19231E,
        elevated = 0x223029,
        text = 0xF4F7F4,
        muted = 0x9EACA4,
        accent = 0x8DD7A9,
        accent_soft = 0x274738,
        warning = 0xF0BB62,
        danger = 0xF08A82,
        outline = 0x35443C,
    },
    space = { xs = 4, sm = 8, md = 12, lg = 18, xl = 24 },
    radius = { control = 16, card = 22, sheet = 28 },
    touch = { compact = 48, standard = 56, prominent = 64 },
}

return theme
