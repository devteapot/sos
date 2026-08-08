-- Deliberately invalid experience used to prove bounded candidate evaluation.
return {
    api_version = 3,
    render = function()
        while true do end
    end,
}
