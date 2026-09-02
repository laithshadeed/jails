-- Filetype detection for JDL, the jails application-model authoring language.
--
-- This is load-bearing for more than colour. github/copilot.vim disables
-- itself in any buffer whose filetype is empty (`s:filetype_defaults` maps
-- '.', its stand-in for no filetype, to 0), and Neovim ships no `.jdl`
-- detection of its own -- so before this file, Copilot was silently off in
-- every model.jdl. Naming the filetype is what turns it back on, and the
-- name also becomes the `languageId` copilot.vim sends to the Copilot LSP.
--
-- `.jdl` is not ours alone: JHipster's unrelated JDL uses the same extension.
-- The path jails owns is claimed outright; any other `.jdl` is claimed only
-- when it opens with the `jdl <version>` header that JDL v1 S5.1 requires,
-- and otherwise falls through so another plugin can have it.

local function is_jails_jdl(bufnr)
  local lines = vim.api.nvim_buf_get_lines(bufnr, 0, 32, false)
  for _, line in ipairs(lines) do
    -- Blank lines and `//` comments are trivia ahead of the header.
    if not line:match('^%s*$') and not line:match('^%s*//') then
      return line:match('^%s*jdl%s+%d+%s*$') ~= nil
    end
  end
  return false
end

vim.filetype.add({
  pattern = {
    ['.*/%.jails/model%.jdl'] = 'jdl',
  },
  extension = {
    jdl = function(_, bufnr)
      if is_jails_jdl(bufnr) then return 'jdl' end
      return nil
    end,
  },
})
