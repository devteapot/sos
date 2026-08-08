-- Deliberately invalid experience used to prove the VM allocation limit.
return {
    api_version = 2,
    render = function()
        local values = {}
        while true do
            table.insert(values, string.rep("x", 1024 * 1024))
        end
    end,
}
