-- jails.nvim: a thin, project-aware Neovim wrapper around the `jails` CLI.
-- The CLI remains the source of truth; this module only supplies editor UX.
local M = {}

local config = {
  command = 'jails',
  terminal_height = 15,
  open_created = true,
  root_markers = { 'pom.xml' },
  keymaps = true,
  java_bundles = {},
}

local STREAMING = {
  -- `why` with no argument starts the application and reads its output, so
  -- it needs the terminal like `run` does. `doctor`/`routes`/`beans` print a
  -- report and exit, and `rename` prompts for confirmation -- all three want
  -- the terminal too, so the whole observation set streams.
  doctor = true,
  why = true,
  routes = true,
  beans = true,
  stats = true,
  notes = true,
  rename = true,
  test = true,
  build = true,
  clean = true,
  run = true,
  check = true,
  fmt = true,
  mvn = true,
  start = true,
  stop = true,
  db = true,
  dbconsole = true,
  console = true,
  c = true,
}

-- The CLI's own vocabulary, read from the CLI.
--
-- These four tables used to be written out by hand, and they drifted exactly
-- as `plan.md` §6.1 predicts a copy will: eight generator kinds and three
-- capabilities reached the binary without the Lua moving, so `:Jails g <Tab>`
-- offered a stale menu -- the worst kind of stale, because a completion list
-- looks complete. `tests/editor.rs` then *pinned* them, which caught the drift
-- after the fact but left the copy there to drift again.
--
-- `jails commands --json` is derived from the same clap definition that parses
-- the arguments, so there is nothing left to keep in step: adding a kind is one
-- edit and this menu follows. Read once per session and cached, because a
-- completion callback runs on every keystroke.
local vocabulary_cache = nil

local function vocabulary()
  if vocabulary_cache then return vocabulary_cache end

  -- A completer that errors is worse than one that offers nothing, so every
  -- failure path lands on an empty vocabulary rather than raising: an older
  -- binary without `commands`, a `jails` that is not on PATH, a malformed
  -- payload. `:Jails doctor` is where a broken install should be reported.
  local empty = { subcommands = {}, kinds = {}, capabilities = {}, options = {} }
  if vim.fn.executable(config.command) == 0 then return empty end

  local out = vim.fn.system({ config.command, 'commands', '--json' })
  if vim.v.shell_error ~= 0 then return empty end

  local ok, decoded = pcall(vim.json.decode, out)
  if not ok or type(decoded) ~= 'table' or type(decoded.subcommands) ~= 'table' then
    return empty
  end

  local names = function(entries)
    local flat = {}
    for _, entry in ipairs(entries or {}) do
      table.insert(flat, entry.name)
      for _, alias in ipairs(entry.aliases or {}) do
        table.insert(flat, alias)
      end
    end
    return flat
  end

  local options = {}
  for _, entry in ipairs(decoded.subcommands) do
    options[entry.name] = entry.options or {}
    for _, alias in ipairs(entry.aliases or {}) do
      options[alias] = entry.options or {}
    end
  end

  vocabulary_cache = {
    subcommands = names(decoded.subcommands),
    kinds = names(decoded.kinds),
    capabilities = names(decoded.capabilities),
    options = options,
  }
  return vocabulary_cache
end

function M.setup(opts)
  config = vim.tbl_deep_extend('force', config, opts or {})
end

function M.project_root()
  local buffer_root = vim.b.jails_root
  if buffer_root and buffer_root ~= '' then return buffer_root end

  local name = vim.api.nvim_buf_get_name(0)
  local start = name ~= '' and vim.fs.dirname(name) or vim.fn.getcwd()
  return vim.fs.root(start, config.root_markers) or vim.fn.getcwd()
end

local function jails_bin()
  if vim.fn.executable(config.command) == 0 then
    vim.notify(
      ('jails.nvim: `%s` not found on PATH'):format(config.command),
      vim.log.levels.ERROR
    )
    return nil
  end
  return config.command
end

local function trim(value)
  return (value or ''):gsub('%s+$', '')
end

local function absolute_path(root, path)
  if path:sub(1, 1) == '/' or path:match('^%a:[/\\]') then return path end
  return vim.fs.joinpath(root, path)
end

local function created_files(output, root)
  local files = {}
  local seen = {}

  for _, line in ipairs(vim.split(output, '\n', { plain = true })) do
    local path
    local root_at = line:find(root, 1, true)
    if root_at then
      path = line:sub(root_at)
    else
      path = line:match('^%s*create%s+(.+)$')
    end

    if path then
      path = vim.fs.normalize(absolute_path(root, path))
      if not path:match('/%.gitkeep$') and vim.fn.filereadable(path) == 1 and not seen[path] then
        seen[path] = true
        table.insert(files, path)
      end
    end
  end

  return files
end

local function open_created_files(output, root)
  local files = created_files(output, root)
  if #files == 0 then return end

  local items = {}
  for _, path in ipairs(files) do
    table.insert(items, { filename = path, lnum = 1, col = 1 })
  end
  vim.fn.setqflist({}, 'r', { title = 'Jails created files', items = items })
  vim.cmd.edit(vim.fn.fnameescape(files[1]))

  if #files > 1 then
    vim.notify(
      ('jails.nvim: opened %s; %d generated files are in the quickfix list (:cnext)'):format(
        vim.fn.fnamemodify(files[1], ':t'),
        #files
      ),
      vim.log.levels.INFO
    )
  end
end

--- Run a short command asynchronously from the nearest Maven project.
function M.run(args, opts)
  local bin = jails_bin()
  if not bin then return end

  opts = opts or {}
  local root = opts.cwd or M.project_root()
  local cmd = { bin }
  vim.list_extend(cmd, args)

  return vim.system(cmd, { cwd = root, text = true }, function(result)
    vim.schedule(function()
      local stdout = trim(result.stdout)
      local stderr = trim(result.stderr)
      if result.code ~= 0 then
        local detail = stderr ~= '' and stderr or stdout
        vim.notify(
          ('jails %s failed%s'):format(
            table.concat(args, ' '),
            detail ~= '' and (':\n' .. detail) or ''
          ),
          vim.log.levels.ERROR
        )
        return
      end

      if stdout ~= '' and opts.notify ~= false then
        vim.notify(stdout, vim.log.levels.INFO)
      end
      if config.open_created and opts.open_created ~= false then
        open_created_files(stdout, root)
      end
    end)
  end)
end

local term_win

--- Run a long command in one reusable bottom terminal, rooted at pom.xml.
function M.run_terminal(args)
  local bin = jails_bin()
  if not bin then return end

  local root = M.project_root()
  local cmd = { bin }
  vim.list_extend(cmd, args)

  if term_win and vim.api.nvim_win_is_valid(term_win) then
    vim.api.nvim_set_current_win(term_win)
  else
    vim.cmd('botright split')
    term_win = vim.api.nvim_get_current_win()
  end
  vim.api.nvim_win_set_height(term_win, config.terminal_height)

  local buffer = vim.api.nvim_create_buf(false, true)
  vim.api.nvim_win_set_buf(term_win, buffer)
  vim.bo[buffer].bufhidden = 'wipe'
  vim.bo[buffer].swapfile = false
  vim.b[buffer].jails_root = root

  vim.fn.jobstart(cmd, {
    cwd = root,
    term = true,
    on_exit = function(_, code)
      if code ~= 0 then
        vim.schedule(function()
          vim.notify(('jails %s exited with status %d'):format(args[1] or '', code), vim.log.levels.ERROR)
        end)
      end
    end,
  })
  vim.cmd.startinsert()
end

--- Merge the settings jdt.ls needs for generator-driven projects into an
--- existing nvim-jdtls config. The caller still owns `cmd` and root
--- discovery; jails only supplies settings, bundles and the heap ceiling.
function M.extend_jdtls(base)
  local result = vim.deepcopy(base or {})
  result.settings = vim.tbl_deep_extend('force', result.settings or {}, {
    java = {
      configuration = { updateBuildConfiguration = 'automatic' },
      autobuild = { enabled = true },
      maven = { downloadSources = true },
      eclipse = { downloadSources = true },
      debug = { settings = { hotCodeReplace = 'auto' } },
    },
  })
  result.init_options = result.init_options or {}
  result.init_options.bundles = vim.list_extend(
    vim.deepcopy(result.init_options.bundles or {}),
    vim.deepcopy(config.java_bundles or {})
  )
  if result.cmd and not vim.tbl_contains(result.cmd, '--jvm-arg=-Xmx2G') then
    table.insert(result.cmd, '--jvm-arg=-Xmx2G')
  end
  return result
end

local function current_test_selector()
  local file = vim.api.nvim_buf_get_name(0)
  if file == '' then return nil end
  return ('%s:%d'):format(file, vim.api.nvim_win_get_cursor(0)[1])
end

function M.configure_java_buffer()
  local root = M.project_root()
  for _, source in ipairs({ 'src/main/java', 'src/test/java' }) do
    vim.opt_local.path:append(vim.fs.joinpath(root, source))
  end
  for _, source in ipairs(vim.g.ftplugin_java_source_path or {}) do
    vim.opt_local.path:append(source)
  end
  vim.cmd.compiler('jails')

  if not config.keymaps then return end
  local map = function(lhs, rhs, desc)
    vim.keymap.set('n', lhs, rhs, { buffer = true, silent = true, desc = desc })
  end
  map('<leader>Jt', function()
    local selector = current_test_selector()
    if selector then M.run_terminal({ 'test', selector }) end
  end, 'Jails: test at cursor')
  map('<leader>Jc', function() M.run_terminal({ 'check' }) end, 'Jails: clean verify')
  map('<leader>Jr', function() M.run_terminal({ 'run' }) end, 'Jails: run')
  map('<leader>Jb', function() M.run_terminal({ 'build' }) end, 'Jails: build')

  local ok, dap = pcall(require, 'jdtls.dap')
  if ok then
    map('<leader>jt', dap.test_nearest_method, 'Java: test nearest method in jdt.ls')
    map('<leader>jc', dap.test_class, 'Java: test class in jdt.ls')
  end
end

function M.destroy(args)
  if #args < 2 then
    vim.notify('jails.nvim: usage :Jails destroy <kind> <Name>', vim.log.levels.ERROR)
    return
  end

  local choice = vim.fn.confirm(('destroy %s?'):format(table.concat(args, ' ')), '&Yes\n&No', 2)
  if choice ~= 1 then return end

  local command = { 'destroy' }
  vim.list_extend(command, args)
  if not vim.tbl_contains(command, '--force') then table.insert(command, '--force') end
  M.run(command)
end

function M.remove(args)
  if #args < 1 then
    vim.notify('jails.nvim: usage :Jails remove <capability>...', vim.log.levels.ERROR)
    return
  end

  local choice = vim.fn.confirm(('remove %s?'):format(table.concat(args, ' ')), '&Yes\n&No', 2)
  if choice ~= 1 then return end

  local command = { 'remove' }
  vim.list_extend(command, args)
  if not vim.tbl_contains(command, '--force') then table.insert(command, '--force') end
  M.run(command)
end

function M.dispatch(fargs)
  if #fargs == 0 then
    vim.notify(
      'jails.nvim: :Jails <g|a|d|test|check|run> ... (use :Jails help for everything)',
      vim.log.levels.INFO
    )
    return
  end

  local sub = fargs[1]
  if sub == 'destroy' or sub == 'd' then
    M.destroy(vim.list_slice(fargs, 2))
  elseif sub == 'remove' or sub == 'rm' then
    M.remove(vim.list_slice(fargs, 2))
  elseif STREAMING[sub] then
    M.run_terminal(fargs)
  else
    M.run(fargs)
  end
end

local function matching(values, lead)
  if not lead or lead == '' then return values end
  local matches = {}
  for _, value in ipairs(values) do
    if value:sub(1, #lead) == lead then table.insert(matches, value) end
  end
  return matches
end

local function test_names()
  local root = M.project_root()
  local dir = vim.fs.joinpath(root, 'src/test/java')
  if vim.fn.isdirectory(dir) == 0 then return {} end

  local names = {}
  local seen = {}
  for _, path in ipairs(vim.fs.find(function(name)
    return name:match('Tests?%.java$') or name:match('IT%.java$')
  end, { path = dir, type = 'file', limit = 500 })) do
    local name = vim.fn.fnamemodify(path, ':t:r')
    if not seen[name] then
      seen[name] = true
      table.insert(names, name)
    end
  end
  table.sort(names)
  return names
end

--- Every type the project declares, by filename. `jails rename` takes a
--- simple type name and refuses a package-qualified one, so a filename stem
--- is exactly the right shape -- and typing the old name from memory is the
--- step most likely to be got wrong.
local function type_names()
  local root = M.project_root()
  local names = {}
  local seen = {}
  for _, sub in ipairs({ 'src/main/java', 'src/test/java' }) do
    local dir = vim.fs.joinpath(root, sub)
    if vim.fn.isdirectory(dir) == 1 then
      for _, path in ipairs(vim.fs.find(function(name)
        return name:match('%.java$')
      end, { path = dir, type = 'file', limit = 1000 })) do
        local name = vim.fn.fnamemodify(path, ':t:r')
        if not seen[name] then
          seen[name] = true
          table.insert(names, name)
        end
      end
    end
  end
  table.sort(names)
  return names
end

--- Complete subcommands, generator kinds, capabilities, options and test names.
function M.complete(arg_lead, cmd_line)
  local words = vim.split(cmd_line, '%s+', { trimempty = true })
  table.remove(words, 1) -- :Jails
  local position = #words + (cmd_line:match('%s$') and 1 or 0)

  if position <= 1 then return matching(vocabulary().subcommands, arg_lead) end

  local sub = words[1]
  if position == 2 then
    if sub == 'generate' or sub == 'g' or sub == 'destroy' or sub == 'd' then
      return matching(vocabulary().kinds, arg_lead)
    end
    if sub == 'add' or sub == 'a' or sub == 'remove' or sub == 'rm' then
      return matching(vocabulary().capabilities, arg_lead)
    end
    if sub == 'start' or sub == 'stop' then
      return matching(RUNTIMES, arg_lead)
    end
    if sub == 'test' then return matching(test_names(), arg_lead) end
    -- Only the first argument: the second is the *new* name, which by
    -- definition does not exist yet and must not be completed from what does.
    if sub == 'rename' then return matching(type_names(), arg_lead) end
  end

  return matching(vocabulary().options[sub] or {}, arg_lead)
end

return M
