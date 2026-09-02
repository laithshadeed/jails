//! `jails generate`'s own argument surface.
//!
//! Its own module because `main.rs` is dispatch only: fields on the
//! `Command::Generate` variant would have to be named twice there, once to
//! destructure and once to rebuild, and twenty arguments on one variant is a
//! long parameter list wearing a `match` arm.

use super::*;

/// Everything `jails generate` takes, as one `clap::Args` struct.
///
/// clap renders a single-field subcommand variant whose type derives `Args`
/// as exactly the arguments the fields declare, so `jails generate --help`
/// and the `commands` walk over the same `clap::Command` see no difference.
// `Debug` because `.jails/app.toml`'s parser produces this type rather than a
// manifest-shaped copy of it, and its tests assert on the errors a malformed
// row produces -- `unwrap_err` needs to be able to render the Ok side.
// `Clone` for the same reason `Invocation` has it: a replay hands the same
// row to a frontend that takes it by value.
#[derive(Clone, Debug, clap::Args)]
pub(crate) struct GenerateArgs {
    pub(crate) kind: ArtifactKind,
    pub(crate) name: String,
    pub(crate) fields: Vec<String>,
    /// Add conventional `createdAt` and `updatedAt` instant components.
    /// The generated create path supplies both; transitions advance
    /// `updated_at` in the same optimistic SQL statement.
    #[arg(long)]
    pub(crate) timestamps: bool,
    /// Subpackage to place the generated code in, relative to the base
    /// package -- overrides the conventional one for the kind. Pass an
    /// empty string to write straight into the base package.
    #[arg(long)]
    pub(crate) package: Option<String>,
    /// Typed value used to backfill rows for `generate field`
    #[arg(long, conflicts_with = "backfill_file")]
    pub(crate) default_literal: Option<String>,
    /// Project-relative reader-owned SQL used by `generate field`
    #[arg(long, conflicts_with = "default_literal")]
    pub(crate) backfill_file: Option<String>,
    /// A composite or ordered index for the generated migration, as the
    /// column list Postgres wants. Repeatable.
    ///
    /// Per-column `@index` covers the single-column case; this is for the
    /// ones it cannot spell:
    ///   --index 'customer_id, created_at desc'
    #[arg(long = "index", value_name = "COLUMNS")]
    pub(crate) indexes: Vec<String>,
    /// A composite unique key on the generated table, as the comma-separated
    /// component list it covers.
    ///
    /// Per-column `@unique` covers the single-column case; this is for the
    /// composite one, and it is what a multi-tenant foreign key needs:
    /// PostgreSQL requires the columns a reference names to carry a unique
    /// constraint of their own, so `(workspaceId, id)` needs stating even
    /// where `id` alone is already the key.
    ///
    ///   --unique 'workspaceId, id'
    #[arg(long = "unique", value_name = "COMPONENTS")]
    pub(crate) uniques: Vec<String>,
    /// For `strategy`, the type each implementation examines. For
    /// `usecase`, the existing scaffolded resource the operation creates;
    /// for `query`, the scaffolded resource it reads; for `durable-job`,
    /// the existing generated use case it invokes. For `command`, the
    /// dispatcher to register it in, when the project has more than one.
    ///
    ///   jails g strategy RewardRule Coffee Large --on Transaction --yields Reward
    #[arg(long = "on", value_name = "TYPE")]
    pub(crate) strategy_on: Option<String>,
    /// For `strategy`: what a matching implementation produces. Omit and
    /// the strategy is a predicate returning `boolean`. For
    /// `durable-job`, the resource whose stable id proves completion.
    #[arg(long = "yields", visible_alias = "returns", value_name = "TYPE")]
    pub(crate) strategy_yields: Option<String>,
    /// For `query`, a second resource to read alongside `--on`, so a
    /// filter may name a component of either.
    ///
    ///   jails g query UnreadForEmail email:string! read:boolean --on Message --via User
    ///
    /// The join column is the child component that references the
    /// parent's key -- `<parent>Id` when it is there, otherwise the one
    /// component of the parent key's type whose name ends in `Id`. Two
    /// candidates is a refusal naming both, never a choice.
    #[arg(long = "via", value_name = "TYPE")]
    pub(crate) via: Option<String>,
    /// For `query`, the result order, as components of `--on` (or the
    /// column names they map to), each optionally `asc`/`desc`.
    ///
    ///   jails g query RecentMessages userId:long --on Message --order-by 'sentAt desc'
    ///
    /// Omit and the adapter orders newest first with the key as the
    /// tiebreak, which is what it has always done.
    #[arg(long = "order-by", value_name = "COMPONENTS")]
    pub(crate) order_by: Option<String>,
    /// For `query`, the row ceiling. Defaults to 100.
    #[arg(long, value_name = "ROWS")]
    pub(crate) limit: Option<u32>,
    /// For `usecase`, the target component whose unique constraint turns
    /// the create into a get-or-create.
    ///
    ///   jails g usecase EnsureUser email:string! --on User --on-conflict email
    ///
    /// One `insert ... on conflict (col) do nothing returning`, then a
    /// read of the row that was already there. The component must be
    /// declared `@unique` or `@pk` on the target -- Postgres has nothing
    /// to conflict on otherwise -- and must be one the command carries.
    #[arg(long = "on-conflict", value_name = "COMPONENT")]
    pub(crate) on_conflict: Option<String>,
    /// The route a generated endpoint answers, instead of the derived one.
    ///
    ///   jails g usecase Ping email:string! --on User --path /customer_api/ping
    ///
    /// Derived paths are a virtue greenfield; they are unusable when the
    /// URLs are a fixed external contract. Valid on `controller`,
    /// `scaffold`, `usecase`, `query` and `transition`.
    ///
    /// On a scaffold it names the *collection*, and the item routes hang
    /// off it: `--path /admin_api/users` also serves
    /// `GET /admin_api/users/{id}`.
    #[arg(long, value_name = "PATH")]
    pub(crate) path: Option<String>,
    /// Which component identifies the row a `transition` updates.
    ///
    ///   jails g transition SetStatus --on Conversation --select userId ...
    ///
    /// `id` by default. A path variable binds to this component, so
    /// `--path /admin_api/conversations/{userId}/status --select userId`
    /// takes the key from the URL and the rest from the body.
    #[arg(long, value_name = "FIELD")]
    pub(crate) select: Option<String>,
    /// Pin one component to a constant instead of reading it from the
    /// request. Repeatable, as `component=literal`.
    ///
    ///   jails g usecase SendAdminMessage userId:long content:string! \
    ///     --on Message --set senderType=ADMIN
    ///
    /// The endpoint that must write `ADMIN` and the one that must write
    /// `CUSTOMER` are two endpoints, and with the component in the request
    /// either can forge the other's rows -- a well-formed request is
    /// exactly what the forgery looks like, so no validation on the
    /// request closes it.
    ///
    /// The value is a literal, never a Java expression: an enum constant,
    /// a boolean, a number, or a short piece of text. It is resolved
    /// against the component's declared type, so a constant that is not
    /// one of that enum's constants is refused by name rather than
    /// written into your code.
    ///
    /// Valid on `usecase` and `transition`. A `transition` whose every
    /// mutated component is pinned needs no version and no `If-Match`:
    /// every writer writes the same value, so there is no update to lose.
    #[arg(long = "set", value_name = "COMPONENT=VALUE")]
    pub(crate) set: Vec<String>,
    /// For `transition`, whether the caller's `If-Match` is insisted on.
    /// Defaults to `required`.
    ///
    ///   jails g transition MarkRead --on Message --if-match optional ...
    ///
    /// `If-Match` is a conditional request header: RFC 9110 has the server
    /// evaluate it when it is present, and requiring it is a policy. It is
    /// jails' default policy, because the compare-and-swap is what a
    /// transition is. `optional` makes the update unconditional when no
    /// precondition arrives and conditional when one does -- which is what
    /// an ordinary browser page needs, since `$.ajax({type: 'PATCH'})`
    /// sends no header and Spring answers 400 for a missing required one
    /// before any generated code runs.
    #[arg(long = "if-match", value_name = "POLICY")]
    pub(crate) if_match: Option<jails_model::Precondition>,
    /// Bind one component from a request parameter of a different name.
    /// Repeatable, as `component=parameter`.
    ///
    ///   jails g transition MarkRead id:long --on Message --bind id=message_id
    ///
    /// Spring's data binder has no naming strategy: Jackson has one and
    /// applies it to JSON without help, so a project whose responses are
    /// snake_case still binds a *form* field called `userId` unless the
    /// component says otherwise. jails derives that from the project's Jackson
    /// setting, and derivation cannot cover a value that is `id` in the
    /// response and `message_id` in the request -- neither name follows from
    /// the other.
    ///
    /// Only meaningful with `--consumes form`, and refused without it.
    #[arg(long = "bind", value_name = "COMPONENT=PARAMETER")]
    pub(crate) bind: Vec<String>,
    /// For `controller`, the HTTP method the generated route answers.
    /// Defaults to `get`.
    ///
    ///   jails g controller Verify --method post --returns Verification
    ///
    /// `--on <Type>` becomes the `@RequestBody` parameter on a verb that
    /// carries one; `--returns <Type>` is what the handler returns.
    #[arg(long, value_name = "METHOD")]
    pub(crate) method: Option<jails_model::EndpointMethod>,
    /// How the generated endpoint reads its request. Defaults to `json`.
    ///
    ///   jails g usecase Ping email:string! --on User --consumes form
    ///
    /// `form` is `application/x-www-form-urlencoded` -- what an HTML form
    /// and jQuery's `$.post(url, object)` send, and what a `@RequestBody`
    /// endpoint answers 415 to. Valid on `controller`, `usecase`,
    /// `query` and `transition`.
    #[arg(long, value_name = "FORMAT")]
    pub(crate) consumes: Option<jails_model::RequestFormat>,
}
