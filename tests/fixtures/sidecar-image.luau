return {
    api_version = 4,
    exports = { main = { render = function()
        return {
            id = "sidecar-proof",
            layout = {
                flow = "column",
                width = 420,
                height = 420,
                padding = 24,
                gap = 16,
            },
            paint = {
                { kind = "fill_bounds", color = 0x171A20, radius = 28 },
            },
            children = {
                {
                    id = "sidecar-title",
                    content = {
                        kind = "text",
                        value = "Supervisor sidecar → Linux host",
                        size = 22,
                        color = 0xF5F1E8,
                    },
                },
                {
                    id = "sidecar-image",
                    layout = { width = 256, height = 256 },
                    content = { kind = "image", asset = "hero" },
                },
            },
        }
    end } },
}
