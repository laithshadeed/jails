-- jails.nvim: thin Neovim wrapper around the `jails` CLI. Replaces
-- springgen.nvim now that jails itself does all the generation (Rust,
-- dependency-free templates) -- this plugin's only job is shelling out to
-- the real binary and giving the result a decent editor UX, not
-- reimplementing any of jails' own logic.
local M = {}

--- Streaming subcommands get a live terminal instead of buffered output --
--- mvn/mvnd output is verbose and users may want to watch it or Ctrl-C it.
local STREAMING = { test = true, build = true, run = true, check = true, fmt = true }

local function jails_bin()
  if vim.fn.executable('jails') == 0 then
    vim.notify('jails.nvim: `jails` binary not found on PATH', vim.log.levels.ERROR)
    return nil
  end
  return 'jails'
end

--- Parse jails' own `created <kind> <path>` lines and open each file.
local function open_created_files(output)
  for _, line in ipairs(vim.split(output, '\n')) do
    local rest = line:match('^created%s+(.+)$')
    local path = rest and rest:match('%S+$')
    if path and vim.fn.filereadable(path) == 1 then
      vim.cmd.edit(path)
    end
  end
end

--- Run a quick filesystem subcommand (generate, destroy, new, new-cli):
--- async, buffered output, notify on completion, open any created files.
function M.run(args)
  local bin = jails_bin()
  if not bin then return end
  local cmd = { bin }
  vim.list_extend(cmd, args)
  vim.system(cmd, { text = true }, function(result)
    vim.schedule(function()
      if result.code ~= 0 then
        vim.notify('jails ' .. table.concat(args, ' ') .. ' failed:\n' .. (result.stderr or ''), vim.log.levels.ERROR)
        return
      end
      local out = (result.stdout or ''):gsub('%s+$', '')
      if out ~= '' then vim.notify(out, vim.log.levels.INFO) end
      if args[1] == 'generate' or args[1] == 'g' then
        open_created_files(out)
      end
    end)
  end)
end

-- Fixed bottom output panel for streaming commands, reused across calls
-- instead of stacking a fresh split every invocation.
local term_win = nil

--- Run a streaming subcommand (test, build, run) in a shared terminal
--- panel: same window every time, a fresh buffer/job per run.
function M.run_terminal(args)
  local bin = jails_bin()
  if not bin then return end
  local cmd = { bin }
  vim.list_extend(cmd, args)

  if term_win and vim.api.nvim_win_is_valid(term_win) then
    vim.api.nvim_set_current_win(term_win)
  else
    vim.cmd('botright split')
    vim.api.nvim_win_set_height(0, 15)
    term_win = vim.api.nvim_get_current_win()
  end
  -- Unconditional: :split clones whichever buffer is currently focused
  -- into the new window, so without this a leftover terminal buffer from
  -- an earlier run could get cloned into the new split and displayed
  -- alongside the reused one -- always start from a blank scratch buffer
  -- before termopen, regardless of which branch above ran.
  vim.cmd.enew()
  vim.fn.termopen(cmd)
  vim.cmd.startinsert()
end

--- destroy needs a yes/no, but piping stdin through an async job is
--- fiddly -- Neovim owns the confirmation instead, then always passes
--- --force since the confirmation already happened.
function M.destroy(kind, name)
  local choice = vim.fn.confirm(('destroy %s %s?'):format(kind, name), '&Yes\n&No', 2)
  if choice ~= 1 then return end
  M.run({ 'destroy', kind, name, '--force' })
end

--- Entry point for :Jails <fargs...>.
function M.dispatch(fargs)
  if #fargs == 0 then
    vim.notify('jails.nvim: usage :Jails <new|new-cli|generate|g|add|a|destroy|d|test|build|check|fmt|run> ...', vim.log.levels.ERROR)
    return
  end
  local sub = fargs[1]
  if sub == 'destroy' or sub == 'd' then
    if #fargs < 3 then
      vim.notify('jails.nvim: usage :Jails destroy <kind> <Name>', vim.log.levels.ERROR)
      return
    end
    M.destroy(fargs[2], fargs[3])
    return
  end
  if STREAMING[sub] then
    M.run_terminal(fargs)
    return
  end
  M.run(fargs)
end

-- Hand-maintained mirrors of jails' own ValueEnums: a new kind or capability
-- has to be added here too, or it silently won't complete. (`jails completion
-- bash` derives its list from clap and cannot drift; this one can.)
local KINDS =
  { 'scaffold', 'controller', 'service', 'repository', 'entity', 'record', 'value', 'enum', 'sealed', 'command', 'cli', 'cases', 'test' }
local CAPABILITIES = { 'csv', 'sqlite', 'json', 'testkit', 'fake', 'http', 'format' }
local SUBCOMMANDS = {
  'new',
  'new-cli',
  'generate',
  'g',
  'add',
  'a',
  'destroy',
  'd',
  'test',
  'build',
  'fmt',
  'check',
  'run',
  'completion',
}

--- Completion for :Jails -- subcommand first, then the artifact kind for
--- generate/destroy or the capability for add, nothing after that (Name and
--- fields are free text). `cmd_line` is the full command line up to the
--- cursor, e.g. "Jails generate " or "Jails g sca".
function M.complete(_, cmd_line)
  local args = vim.split(vim.trim(cmd_line), '%s+')
  table.remove(args, 1) -- drop the "Jails" command name itself
  local completed = #args
  if not cmd_line:match('%s$') and completed > 0 then
    completed = completed - 1 -- last word is still being typed
  end
  if completed == 0 then return SUBCOMMANDS end
  local sub = args[1]
  if completed == 1 then
    if sub == 'generate' or sub == 'g' or sub == 'destroy' or sub == 'd' then return KINDS end
    if sub == 'add' or sub == 'a' then return CAPABILITIES end
  end
  return {}
end

return M
