-- jails.nvim: a thin, project-aware Neovim wrapper around the `jails` CLI.
-- The CLI remains the source of truth; this module only supplies editor UX.
local M = {}

local config = {
  command = 'jails',
  terminal = { height = 12 },
  output_schema = 'v2',
  diagnostics = { enabled = true, on_save = 'offline' },
  watch = { auto_start = false, statusline = true, compile = false },
  open_created = true,
  root_markers = {
    '.jails/app.toml', 'pom.xml', 'mvnw', 'build.gradle',
    'build.gradle.kts', 'gradlew', '.git',
  },
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
local vocabulary_loading = false

local function vocabulary()
  if vocabulary_cache then return vocabulary_cache end

  -- A completer that errors is worse than one that offers nothing, so every
  -- failure path lands on an empty vocabulary rather than raising: an older
  -- binary without `commands`, a `jails` that is not on PATH, a malformed
  -- payload. `:Jails doctor` is where a broken install should be reported.
  local empty = { subcommands = {}, kinds = {}, capabilities = {}, options = {} }
  if vim.fn.executable(config.command) == 0 or vocabulary_loading then return empty end
  vocabulary_loading = true

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

  vim.system({ config.command, 'commands', '--json' }, { text = true }, function(result)
    vocabulary_loading = false
    if result.code ~= 0 then return end
    local ok, decoded = pcall(vim.json.decode, result.stdout or '')
    if not ok or type(decoded) ~= 'table' or type(decoded.subcommands) ~= 'table' then return end
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
  end)
  return empty
end

function M.setup(opts)
  opts = opts or {}
  if opts.terminal_height ~= nil then
    if opts.terminal ~= nil and opts.terminal.height ~= nil then
      error('jails.nvim: terminal_height conflicts with terminal.height')
    end
    opts.terminal = { height = opts.terminal_height }
    opts.terminal_height = nil
  end
  local allowed = {
    command = true, terminal = true, output_schema = true, diagnostics = true,
    watch = true, open_created = true, root_markers = true, keymaps = true,
    java_bundles = true,
  }
  for key, _ in pairs(opts) do
    if not allowed[key] then error(('jails.nvim: unknown setup key `%s`'):format(key)) end
  end
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
local function open_receipt_files(receipt, root)
  local files = {}
  local seen = {}
  for _, operation in ipairs((receipt or {}).operations or {}) do
    local path = operation.path
    if path and operation.kind ~= 'delete' then
      path = vim.fs.normalize(absolute_path(root, path))
      if vim.fn.filereadable(path) == 1 and not seen[path] then
        seen[path] = true
        table.insert(files, path)
      end
    end
  end
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
        if opts.callback then opts.callback(nil, detail ~= '' and detail or ('exit ' .. result.code)) end
        return
      end

      if stdout ~= '' and opts.notify ~= false then
        vim.notify(stdout, vim.log.levels.INFO)
      end
      if opts.callback then opts.callback(result, nil) end
    end)
  end)
end

function M.health(callback)
  local result = {
    executable = config.command,
    available = vim.fn.executable(config.command) == 1,
    output_schema = config.output_schema,
  }
  if callback then callback(result, result.available and nil or 'tool-unavailable') end
  return result
end

local function decode_result(result, expected_schema)
  if result.code ~= 0 then return nil, trim(result.stderr) end
  local ok, decoded = pcall(vim.json.decode, result.stdout or '')
  if not ok or type(decoded) ~= 'table' then return nil, 'malformed JSON response' end
  if expected_schema and decoded.schema ~= expected_schema then
    return nil, ('protocol-mismatch: expected %s, received %s'):format(
      expected_schema,
      tostring(decoded.schema)
    )
  end
  return decoded, nil
end

local handshake_cache = {}

--- Negotiate editor capabilities asynchronously. Failed handshakes are never cached.
function M.handshake(callback)
  local bin = jails_bin()
  if not bin then return end
  local root = M.project_root()
  local cached = handshake_cache[root]
  if cached then callback(cached); return end
  vim.system(
    { bin, '--output', 'json', 'editor', 'handshake', '--path', root },
    { cwd = root, text = true },
    function(result)
      local decoded, error = decode_result(result, 'jails.editor-handshake.v1')
      vim.schedule(function()
        if error then
          vim.notify(('jails.nvim: handshake failed: %s'):format(error), vim.log.levels.WARN)
          return
        end
        handshake_cache[root] = decoded
        callback(decoded)
      end)
    end
  )
end

local active_plans = {}

local function remove_plan(plan)
  if not plan then return end
  active_plans[plan.id] = nil
  if plan.directory then vim.fs.rm(plan.directory, { recursive = true, force = true }) end
end

local function prepared_data(envelope)
  if envelope.schema ~= 'jails.command-result.v2' then
    return nil, ('protocol-mismatch: expected jails.command-result.v2, received %s'):format(
      tostring(envelope.schema)
    )
  end
  local report = envelope.report
  if envelope.status ~= 'preview' or not report or report.kind ~= 'prepared'
    or report.schema ~= 'jails.prepared-report.v1' then
    return nil, 'protocol-mismatch: preview did not return a prepared report'
  end
  return report.data, nil
end

local function render_plan(plan)
  local lines = {
    ('Jails prepared plan %s'):format(plan.id),
    ('risk: %s'):format(tostring(plan.data.risk or 'unknown')),
    ('digest: %s'):format(tostring(plan.data.digest or plan.id)),
    '',
  }
  for _, operation in ipairs(plan.data.operations or {}) do
    table.insert(lines, ('%-8s %s'):format(
      tostring(operation.kind or operation.operation or 'change'):upper(),
      tostring(operation.path or operation.subject or '')
    ))
    for _, reason in ipairs(operation.reasons or {}) do
      table.insert(lines, ('  because %s'):format(reason))
    end
    if operation.diff then
      for line in tostring(operation.diff):gmatch('[^\n]+') do table.insert(lines, line) end
    end
  end
  local buffer = vim.api.nvim_create_buf(false, true)
  vim.api.nvim_buf_set_name(buffer, 'jails://plan/' .. plan.id)
  vim.bo[buffer].buftype = 'nofile'
  vim.bo[buffer].bufhidden = 'wipe'
  vim.bo[buffer].swapfile = false
  vim.bo[buffer].modifiable = true
  vim.api.nvim_buf_set_lines(buffer, 0, -1, false, lines)
  vim.bo[buffer].modifiable = false
  vim.api.nvim_set_current_buf(buffer)
  plan.buffer = buffer
end

function M.preview(args, callback)
  local bin = jails_bin()
  if not bin then return end
  local root = M.project_root()
  local directory = vim.fn.tempname()
  local ok, error = vim.uv.fs_mkdir(directory, 448)
  if not ok then
    if callback then callback(nil, error) end
    return
  end
  local file = vim.fs.joinpath(directory, 'prepared-plan.json')
  local command = { bin, '--pretend', '--output', 'json', '--plan-out', file }
  vim.list_extend(command, args)
  return vim.system(command, { cwd = root, text = true }, function(result)
    local envelope, decode_error = decode_result(result, 'jails.command-result.v2')
    if not envelope then
      vim.fs.rm(directory, { recursive = true, force = true })
      vim.schedule(function()
        if callback then callback(nil, decode_error) end
        vim.notify(('jails.nvim: preview failed: %s'):format(decode_error), vim.log.levels.ERROR)
      end)
      return
    end
    local data, protocol_error = prepared_data(envelope)
    if not data then
      vim.fs.rm(directory, { recursive = true, force = true })
      vim.schedule(function()
        if callback then callback(nil, protocol_error) end
        vim.notify(('jails.nvim: preview failed: %s'):format(protocol_error), vim.log.levels.ERROR)
      end)
      return
    end
    local id = tostring(data.operation_id or data.digest)
    local plan = {
      id = id,
      root = root,
      directory = directory,
      file = file,
      data = data,
      command_path = envelope.command and envelope.command.path or nil,
    }
    active_plans[id] = plan
    vim.schedule(function()
      render_plan(plan)
      if callback then callback(plan, nil) end
    end)
  end)
end

function M.apply_plan(plan_or_id, callback)
  local plan = type(plan_or_id) == 'table' and plan_or_id or active_plans[plan_or_id]
  if not plan then
    if callback then callback(nil, 'stale plan') end
    return
  end
  local summary = ('Apply plan %s (%s risk)?'):format(
    tostring(plan.data.digest or plan.id), tostring(plan.data.risk or 'unknown')
  )
  vim.ui.select({ 'Apply', 'Cancel' }, { prompt = summary }, function(choice)
    if choice ~= 'Apply' then
      remove_plan(plan)
      if callback then callback(nil, 'cancelled') end
      return
    end
    local command = { config.command, '--output', 'json', '--yes', '--plan-in', plan.file }
    vim.list_extend(command, plan.command_path or {})
    vim.system(command, { cwd = plan.root, text = true }, function(result)
      local envelope, error = decode_result(result, 'jails.command-result.v2')
      vim.schedule(function()
        if envelope and envelope.receipt then
          if config.open_created then open_receipt_files(envelope.receipt, plan.root) end
          if callback then callback(envelope, nil) end
        else
          if callback then callback(nil, error) end
          vim.notify(('jails.nvim: apply failed: %s'):format(error), vim.log.levels.ERROR)
        end
        remove_plan(plan)
      end)
    end)
  end)
end

local function editor_tokens(cmd_line)
  local words = vim.split(cmd_line, '%s+', { trimempty = true })
  table.remove(words, 1)
  return words
end

local completion_cache = {}
local completion_jobs = {}

local function editor_completion(arg_lead, cmd_line)
  local root = M.project_root()
  local words = editor_tokens(cmd_line)
  local position = math.max(#words - 1, 0)
  local offset = #arg_lead
  local key = table.concat({ root, tostring(position), tostring(offset), table.concat(words, '\0') }, '\1')
  if completion_cache[key] then return completion_cache[key] end
  if completion_jobs[key] then return nil end
  local cmd = {
    config.command, '--output', 'json', 'editor', 'complete',
    '--arg-index', tostring(position), '--byte-offset', tostring(offset), '--',
  }
  vim.list_extend(cmd, words)
  completion_jobs[key] = vim.system(cmd, { cwd = root, text = true }, function(result)
    completion_jobs[key] = nil
    if result.code ~= 0 then return end
    local ok, decoded = pcall(vim.json.decode, result.stdout or '')
    if not ok or decoded.schema ~= 'jails.editor-completion.v1' then return end
    local values = {}
    for _, candidate in ipairs(decoded.candidates or {}) do table.insert(values, candidate.value) end
    completion_cache[key] = values
  end)
  return nil
end

local diagnostics_namespace = vim.api.nvim_create_namespace('jails')
local latest_epoch = {}

local function diagnostic_severity(value)
  if value == 'error' then return vim.diagnostic.severity.ERROR end
  if value == 'warning' then return vim.diagnostic.severity.WARN end
  return vim.diagnostic.severity.INFO
end

local function publish_diagnostics(root, report)
  local previous = latest_epoch[root] or 0
  if report.epoch < previous then return end
  latest_epoch[root] = report.epoch
  local grouped = {}
  for _, item in ipairs(report.diagnostics or {}) do
    local primary = item.primary
    if primary and primary.path then
      local path = vim.fs.normalize(absolute_path(root, primary.path))
      grouped[path] = grouped[path] or {}
      local range = primary.range or {}
      local start = range.start or {}
      local finish = range['end'] or start
      table.insert(grouped[path], {
        lnum = start.line or 0,
        col = start.byte_column or 0,
        end_lnum = finish.line or start.line or 0,
        end_col = finish.byte_column or start.byte_column or 0,
        severity = diagnostic_severity(item.severity),
        source = 'jails',
        code = item.code,
        message = item.message,
        user_data = { evidence = item.evidence, fixes = item.fixes },
      })
    end
  end
  for path, items in pairs(grouped) do
    local buffer = vim.fn.bufnr(path, false)
    if buffer >= 0 and vim.api.nvim_buf_is_loaded(buffer) then
      vim.diagnostic.set(diagnostics_namespace, buffer, items, {})
    end
  end
end

function M.diagnostics(scope)
  local bin = jails_bin()
  if not bin then return end
  local root = M.project_root()
  local args = { bin, '--output', 'json', 'editor', 'diagnostics', '--scope', scope or 'project' }
  if scope == 'buffer' then
    local file = vim.api.nvim_buf_get_name(0)
    table.insert(args, '--file')
    table.insert(args, vim.fs.relpath(root, file))
  end
  vim.system(args, { cwd = root, text = true }, function(result)
    local report, error = decode_result(result, 'jails.editor-diagnostics.v1')
    vim.schedule(function()
      if error then vim.notify(('jails.nvim: diagnostics failed: %s'):format(error), vim.log.levels.ERROR)
      else publish_diagnostics(root, report) end
    end)
  end)
end

local watches = {}

local function watch_event(name, root, watch, extra)
  local data = {
    root_digest = watch.root_digest,
    session = watch.session,
    epoch = watch.epoch or 0,
  }
  for key, value in pairs(extra or {}) do data[key] = value end
  vim.api.nvim_exec_autocmds('User', { pattern = name, data = data })
end

function M.watch_status(root)
  local watch = watches[root or M.project_root()]
  return watch and watch.status or 'cold'
end

--- Decode jails.event.v1 incrementally; stdout is protocol-only.
function M.watch_start(root, callback)
  if type(root) == 'function' then callback, root = root, nil end
  root = root or M.project_root()
  if watches[root] then return watches[root].job end
  local buffer, trusted, session, sequence = '', true, nil, -1
  local watch = { status = 'starting', epoch = 0 }
  watches[root] = watch
  watch.job = vim.fn.jobstart(
    { config.command, 'test', '--watch', '--output', 'json' },
    {
      cwd = root,
      stdout_buffered = false,
      on_stdout = function(_, chunks)
        if not trusted then return end
        buffer = buffer .. table.concat(chunks or {}, '\n')
        if #buffer > 8 * 1024 * 1024 then trusted = false; return end
        while true do
          local newline = buffer:find('\n', 1, true)
          if not newline then break end
          local frame = buffer:sub(1, newline - 1)
          buffer = buffer:sub(newline + 1)
          if frame ~= '' then
            local ok, event = pcall(vim.json.decode, frame)
            if not ok or event.schema ~= 'jails.event.v1'
              or (session and event.session ~= session)
              or event.sequence ~= sequence + 1 then
              trusted = false
              vim.schedule(function() vim.notify('jails.nvim: protocol-mismatch in test watch', vim.log.levels.ERROR) end)
              break
            end
            session, sequence = event.session, event.sequence
            watch.session, watch.epoch = event.session, event.epoch
            if event.kind == 'ready' then
              watch.status = 'ready'
              watch_event('JailsWatchReady', root, watch, { current = true })
            elseif event.kind == 'tested' or event.kind == 'testing' then
              watch.status = 'testing'
            elseif event.kind == 'stale' then
              watch.status = 'stale'
              watch_event('JailsWatchReady', root, watch, { current = false })
            elseif event.kind == 'failed' then
              watch.status = 'failed'
            end
            if event.epoch >= (latest_epoch[root] or 0) then latest_epoch[root] = event.epoch end
          end
        end
      end,
      on_exit = function(_, code)
        local owned = watches[root]
        if owned ~= watch then return end
        watches[root] = nil
        watch.status = 'stopped'
        vim.schedule(function()
          watch_event('JailsWatchStopped', root, watch, { reason = code == 0 and 'stopped' or 'failed' })
          if callback then callback(nil, code == 0 and nil or ('exit ' .. code)) end
        end)
      end,
    }
  )
  if watch.job <= 0 then
    watches[root] = nil
    if callback then callback(nil, 'could not start test watch') end
    return nil
  end
  vim.schedule(function()
    watch_event('JailsWatchStarted', root, watch)
    if callback then callback(watch, nil) end
  end)
  return watch.job
end

function M.watch_stop(root, callback)
  if type(root) == 'function' then callback, root = root, nil end
  root = root or M.project_root()
  local watch = watches[root]
  if not watch then
    if callback then callback(nil, nil) end
    return
  end
  local job = watch.job
  vim.fn.jobstop(job)
  vim.defer_fn(function()
    if watches[root] == watch and vim.fn.jobwait({ job }, 0)[1] == -1 then vim.fn.jobstop(job) end
  end, 2000)
  if callback then callback(watch, nil) end
end

function M.watch_toggle(root, callback)
  if type(root) == 'function' then callback, root = root, nil end
  root = root or M.project_root()
  if watches[root] then return M.watch_stop(root, callback) end
  return M.watch_start(root, callback)
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
  vim.api.nvim_win_set_height(term_win, config.terminal.height)

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

function M.test_at_cursor(callback)
  local selector = current_test_selector()
  if not selector then
    if callback then callback(nil, 'no test selector at cursor') end
    return
  end
  return M.run({ 'test', selector, '--output', 'json' }, { callback = callback, notify = false })
end

function M.pick(kind, query, callback)
  local root = M.project_root()
  local command = { config.command, '--output', 'json', 'editor', 'symbols', kind }
  if query and query ~= '' then vim.list_extend(command, { '--query', query }) end
  return vim.system(command, { cwd = root, text = true }, function(result)
    local report, error = decode_result(result, 'jails.editor-symbols.v1')
    vim.schedule(function()
      if not report then
        if callback then callback(nil, error) end
        return
      end
      vim.ui.select(report.symbols or {}, {
        prompt = ('Jails %s'):format(kind),
        format_item = function(item)
          return item.detail and (item.label .. ' — ' .. item.detail) or item.label
        end,
      }, function(item)
        if item and item.location then
          vim.cmd.edit(vim.fn.fnameescape(absolute_path(root, item.location.path)))
          local start = item.location.range and item.location.range.start or {}
          vim.api.nvim_win_set_cursor(0, { (start.line or 0) + 1, start.byte_column or 0 })
        end
        if callback then callback(item, item and nil or 'cancelled') end
      end)
    end)
  end)
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
  -- Primary path: the CLI walks the same Clap graph that parses dispatch.
  -- The cached vocabulary below is a one-release fallback for older binaries.
  local derived = editor_completion(arg_lead, cmd_line)
  if derived then return matching(derived, arg_lead) end

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
