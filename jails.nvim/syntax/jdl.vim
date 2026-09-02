" Vim syntax file for JDL, the jails application-model authoring language.
"
" The vocabulary here is the normative grammar in `docs/01-jdl-v1.md` S6, and
" every word is one the parser in `crates/jails-model/src/jdl/v1/parser/`
" actually matches -- `tests/editor.rs` fails when this file highlights a word
" the parser does not know, because a syntax file that colours a misspelling
" is worse than one that colours nothing.
"
" Language: JDL v1 (jails)
" Filenames: .jails/model.jdl

if exists('b:current_syntax')
  finish
endif

" Kebab spellings (`if-match`, `set-null`, `zone-id`) are `syn match` items
" rather than keywords, so `-` stays out of 'iskeyword' and signed INT/DECIMAL
" literals keep their word boundaries.
syn case match

" -- trivia ------------------------------------------------------------------
" `//` to end of line, outside a string. There are no block comments in v1.
syn keyword jdlTodo contained TODO FIXME XXX NOTE
syn match   jdlComment "//.*$" contains=jdlTodo,@Spell

" -- literals ----------------------------------------------------------------
" JSON escapes: \" \\ \n \r \t \uXXXX. Anything else is a lexical error, so it
" is left unhighlighted rather than coloured as if it were valid.
syn match  jdlEscape  contained "\\\%([\"\\/bfnrt]\|u\x\{4}\)"
syn region jdlString  start=+"+ skip=+\\\\\|\\"+ end=+"+ contains=jdlEscape,@Spell
syn match  jdlNumber  "-\=\<\d\+\%(\.\d\+\)\=\>"
syn keyword jdlBoolean true false
" A range bound may be omitted on either side: @length(1..200), @length(..200).
syn match  jdlRange   "\.\."

" -- document header ---------------------------------------------------------
" `jdl 1` MUST be the first non-comment declaration.
syn match   jdlVersion "^\s*\zsjdl\>\s\+\d\+" contains=jdlNumber

" -- declarations ------------------------------------------------------------
syn keyword jdlDecl app enum entity relation eject variant
syn keyword jdlDecl event

" A PROP_KEY and a COORDINATE are dotted/colon-joined runs that can contain a
" keyword: `prop app.title` would otherwise read `app` as the app declaration.
" Each is reached through nextgroup, because a `\zs` lookbehind match cannot
" start at a position the preceding keyword already claimed.
syn keyword jdlDecl prop nextgroup=jdlPropKey skipwhite
syn keyword jdlDecl dep  nextgroup=jdlCoordinate skipwhite
syn match   jdlPropKey    contained "[A-Za-z0-9_-]\+\%(\.[A-Za-z0-9_-]\+\)*"
syn match   jdlCoordinate contained "[A-Za-z0-9_.-]\+:[A-Za-z0-9_.-]\+"

" `cap`, `use` and `component` are each followed by a lowercase kebab KIND from
" a closed registry, not by an open identifier.
syn match   jdlDecl "\<cap\>"       nextgroup=jdlKind skipwhite
syn match   jdlDecl "\<use\>"       nextgroup=jdlKind skipwhite
syn match   jdlDecl "\<component\>" nextgroup=jdlKind skipwhite
syn match   jdlKind contained "[a-z][a-z0-9]*\%(-[a-z0-9]\+\)*" nextgroup=jdlKindMore skipwhite
syn match   jdlKindMore contained "," nextgroup=jdlKind skipwhite

" -- app properties ----------------------------------------------------------
syn keyword jdlAppKey java platform build storage
" A `\zs` match cannot start at a position a keyword already claimed, so the
" package name is reached through nextgroup rather than a lookbehind on `pkg`.
syn keyword jdlAppKey pkg nextgroup=jdlPackage skipwhite
syn match   jdlPackage contained "[a-z][a-z0-9_]*\%(\.[a-z][a-z0-9_]*\)*"

" -- entity and operation members --------------------------------------------
syn keyword jdlMember command query transition
syn keyword jdlMember table pk unique index
syn keyword jdlMember route bind emit conflict join resolve
syn keyword jdlMember order limit select update partition
syn keyword jdlMember on yields source
syn match   jdlMember "\<if-match\>"
" `set` and `map` are anchored to the start of a logical line: `set` would
" otherwise win over `on delete set-null`, and `map` is also a type
" constructor and the `@map` attribute name.
syn match   jdlMember "^\s*\zsset\>"
syn match   jdlMember "^\s*\zsmap\>"

" -- connectives -------------------------------------------------------------
syn keyword jdlConnective for except to as by where consumes delete
" `bind x from query` and `bind x from path` would otherwise read as the query
" operation kind and the `path` scalar type. nextgroup off `from` scopes the
" five binding sources to exactly that position; in `resolve t from Line.x` the
" next token is not one of them and the rule simply does not fire.
syn keyword jdlConnective from nextgroup=jdlBindSource skipwhite
syn keyword jdlBindSource contained path query header form claim

" -- types -------------------------------------------------------------------
syn keyword jdlType string int long double decimal boolean uuid
syn keyword jdlType date datetime instant duration uri path currency bytes
syn match   jdlType "\<zone-id\>"
syn match   jdlType "\<\%(list\|map\)\>\ze\s*<"
" A trailing `?` is optionality, not part of the type name.
syn match   jdlOptional "?"

" -- closed value vocabularies -----------------------------------------------
syn keyword jdlValue spring plain
syn keyword jdlValue maven gradle
syn keyword jdlValue postgres h2 sqlite none
syn keyword jdlValue json form
syn keyword jdlValue header claim
syn keyword jdlValue asc desc
syn keyword jdlValue restrict cascade
syn match   jdlValue "\<set-null\>"
syn keyword jdlValue required optional
syn keyword jdlHttpMethod GET POST PUT PATCH DELETE

" -- attributes --------------------------------------------------------------
" `@name` and `@name(args)`. The name is consumed by this match, so `@index`
" does not also read as the `index` constraint keyword, and argument words such
" as the dependency scopes stay scoped to the parentheses.
syn match  jdlAttribute "@[a-zA-Z][a-zA-Z0-9_]*" nextgroup=jdlAttrArgs
" The region nests: `@default(now())` closes two parens, and without
" jdlAttrArgs in its own contains the outer region ends at the inner `)`.
syn region jdlAttrArgs matchgroup=jdlDelimiter start="(" end=")" contained
      \ contains=jdlAttrArgs,jdlString,jdlNumber,jdlBoolean,jdlRange,jdlAttrName,
      \jdlAttrScope,jdlConstant,jdlTypeName,jdlAttrValue,jdlAttrCall
syn match  jdlAttrName  contained "[a-zA-Z_][a-zA-Z0-9_]*\ze\s*:"
syn keyword jdlAttrScope contained compile runtime test
" jdlAttrCall is defined after jdlAttrValue so the last-defined-match rule
" gives `now` in `@default(now())` the function reading, not the bare-value one.
syn match  jdlAttrValue contained "[a-z][a-zA-Z0-9_-]*"
syn match  jdlAttrCall  contained "[a-z][a-zA-Z0-9_]*\ze\s*("

" -- identifiers -------------------------------------------------------------
" `PENDING` satisfies both the TYPE_IDENT and the ENUM_IDENT regex. Vim gives
" the last-defined match priority, so ENUM_IDENT -- the specific reading -- is
" defined second and wins.
syn match jdlTypeName "\<[A-Z][A-Za-z0-9]*\>"
syn match jdlConstant "\<[A-Z][A-Z0-9]*\%(_[A-Z0-9]\+\)\+\>"
syn match jdlConstant "\<[A-Z][A-Z0-9]\{1,}\>"
" FIELD_IDENT before a `:`, which covers both a field declaration and an inline
" typed parameter. A parameter named for a keyword (`by:`) still reads as the
" keyword -- Vim ranks keywords above matches, and telling the two apart needs
" the parser, not a regex.
syn match jdlField    "\<[a-z][A-Za-z0-9]*\ze\s*:"

" -- punctuation -------------------------------------------------------------
syn match jdlDelimiter "[{}\[\](),<>]"
syn match jdlOperator  "->"
syn match jdlOperator  "="

" -- links -------------------------------------------------------------------
hi def link jdlComment     Comment
hi def link jdlTodo        Todo
hi def link jdlString      String
hi def link jdlEscape      SpecialChar
hi def link jdlNumber      Number
hi def link jdlBoolean     Boolean
hi def link jdlRange       Operator
hi def link jdlVersion     PreProc
hi def link jdlDecl        Structure
hi def link jdlKind        Identifier
hi def link jdlKindMore    Delimiter
hi def link jdlAppKey      Label
hi def link jdlPackage     Identifier
hi def link jdlPropKey     Identifier
hi def link jdlCoordinate  Identifier
hi def link jdlBindSource  Constant
hi def link jdlMember      Statement
hi def link jdlConnective  Keyword
hi def link jdlType        Type
hi def link jdlOptional    Special
hi def link jdlValue       Constant
hi def link jdlHttpMethod  Special
hi def link jdlAttribute   PreProc
hi def link jdlAttrName    Label
hi def link jdlAttrScope   Constant
hi def link jdlAttrCall    Function
hi def link jdlAttrValue   Identifier
hi def link jdlConstant    Constant
hi def link jdlTypeName    Type
hi def link jdlField       Identifier
hi def link jdlDelimiter   Delimiter
hi def link jdlOperator    Operator

let b:current_syntax = 'jdl'
