-- Deliberately invalid experience used to prove the VM allocation limit.
return {
    api_version = 4,
    exports = { main = { render = function()
        local values = {}
        while true do
            table.insert(values, string.rep("x", 1024 * 1024))
        end
    end } },
}
