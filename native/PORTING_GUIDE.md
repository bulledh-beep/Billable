# Porting a screen to the native app

Read this before porting a page. The React/TypeScript in `../src/renderer` is
the spec: match it **pixel for pixel and behaviour for behaviour**. Don't
redesign, simplify or drop features.

## Ground rules

- **Data safety.** Never open `~/Library/Application Support/Billable` (the
  real data). Run the app only with
  `BILLABLE_USER_DATA="$HOME/Library/Application Support/Billable Dev"`.
- **Own your files only.** Edit only the files your task names. Other people
  are editing the rest at the same time. Need a helper? Put it in your own
  file (private fn, or a submodule like `pages/billing/stamp.rs` declared from
  your page). If a shared file truly must change, say so in your final report
  instead of editing it.
- **The data layer** (`core/`, crate `billable_core`) is someone else's. Use
  its `Db` methods (`core/src/db/*.rs`) and models (`core/src/models.rs`; JSON
  field names match the TypeScript). If a query you need is missing, write it
  in your page against `store::db(cx).connection()` (rusqlite) and report it.
- **Build:** `cd native && cargo build 2>&1 | grep -E "^error" -A 9`. The
  target dir is shared and may be locked by another build for a while; wait,
  don't `cargo clean` or use another target dir.
- **Smoke test:** after it builds, run
  `BILLABLE_USER_DATA="$HOME/Library/Application Support/Billable Dev" BILLABLE_START_ROUTE=/your/path timeout 8 ./target/debug/billable`
  and make sure nothing panics. (Screenshots are done separately.)
- No `TODO`s left behind. If something can't be done in GPUI, do the closest
  thing and list it in your report.

## Converting Tailwind and CSS

- **Spacing is rem-based and Billable's rem is 13px.** One Tailwind unit is
  3.25px: use `r(units)` from `style.rs` (`px-4` → `.px(r(4.))`, `gap-2.5` →
  `.gap(r(2.5))`, `w-9 h-9` → `.size(r(9.))`, `mb-6` → `.mb(r(6.))`).
  Arbitrary values stay in px: `h-[58px]` → `.h(px(58.))`.
- **Font sizes** are px. Named sizes (tailwind.config.js) also set line
  height: `text-2xs` 11/14, `text-xs` 12/16, `text-sm` 13/18, `text-base`
  14/20, `text-lg` 16/22, `text-xl` 18/24, `text-2xl` 22/28, `text-3xl` 28/34.
  Arbitrary sizes (`text-[15px]`) inherit a 1.5 line height (the root sets
  `relative(1.5)`); set `.line_height(px(..))` when the element sets
  `leading-*`.
- **Weights:** body text is 500. `font-medium` 500, `font-semibold` 600,
  `font-bold` 700. Use `text(s, size, weight, color)` or `.weight(600)` (the
  `Weighted` trait), never `font_weight` alone: SF Rounded weights are
  separate font families (see `fonts.rs`).
- `.font-figures` / `.font-display` → `figures(s, size, color)` (bold,
  tabular). `.num` → `.font(num_font(weight))`.
- **Colours:** `theme(cx)` gives the palette: `t.fg t.fg2 t.fg3 t.fg4 t.bg
  t.panel t.panel2 t.field t.line t.line_strong t.accent t.accent_hover
  t.accent_deep t.accent_text t.green t.blue t.red t.amber t.violet t.gray`.
  `bg-fg/[0.06]` → `t.fg.opacity(0.06)`. Tinted mixes: `theme::mix`.
  Hex colours: `theme::hex(0xFF9600)` / `parse_hex("#F5A623")`.
- `.card` → `card(&t)`. `.list-row` separators → `separator(&t, inset)`
  inside a `relative()` row (none on the last row). `.group-head` →
  `group_head(left, right)`. `.section-title` → `section_title`. `.label` →
  `label`. `.hint` → `hint`. `.kbd` → `kbd`. Dots → `dot(color, size)`.
- **Page frame** (`.page`): `div().w_full().max_w(px(1160.)).mx_auto()
  .px(r(7.)).pt(r(6.)).pb(r(14.))`, and blocks rise in with
  `motion::rise_in(id, index, el)` like `.page > *`.
- GPUI can't transform elements: skip `scale`/`rotate`/`translate`
  animations (hover lifts can use `relative().top(px(-2.))`), skip
  `wiggle`, and there's no letter-spacing. Note these, don't fake them badly.

## Components (app/src/components)

- `Button::primary/secondary/ghost/danger(id, label)`,
  `Button::icon_button(id, Lucide::X)`; `.small()`, `.icon(..)`, `.filled()`,
  `.icon_size(px)`, `.disabled(b)`, `.selected(b)`, `.tooltip(..)`,
  `.text_color(..)`, `.hover_text(..)`, `.full_width()`, `.on_click(..)`.
- Text fields hold `Entity<InputState>` (gpui-component) in the page:
  `TextField::new(&state)` (`.leading_icon`, `.leading_text("$")`,
  `.tabular()`, `.trailing(el)`), `TextArea::new(&textarea_state, rows)` with
  `Entity<TextareaState>`, `search_field(id, &state, cx)`. Subscribe to
  `InputEvent::Change` to re-render. Create states with
  `cx.new(|cx| InputState::new(window, cx).placeholder(..).default_value(..))`.
- `select(id, Vec<(value, label)>, &current, on_change, cx)`: the native Mac
  pop-up menu, as Chromium shows for `<select>`.
- `DateInput` (an entity view: `cx.new(|cx| DateInput::date(Some(d), cx))` or
  `DateInput::time(..)`), emits `DateChanged`. Chromium-style segments.
- `fields::checkbox(&t, id, checked, indeterminate)` (add `.on_click`).
- `segmented(id, &value, vec![Segment::new(v, "Label").count(n)], on_change, cx)`.
- `Modal::new(id, title, on_close).size(ModalSize::Sm).description(..)
  .child(body).footer(button)`; `confirm_dialog(...)`.
- Menus: put `Button::icon_button(.., Lucide::MoreHorizontal)` and, while
  open, `popover(id, Align::End, None, menu_list(id, items, close, cx),
  on_close, cx)` in a `relative()` wrapper. `MenuItem::new(label, f)
  .icon(..).danger().hint(..)`; `None` entries are separators.
- `table(&t, window, id, columns, rows, width, empty)` (`components/table.rs`)
  reproduces Chromium's automatic table column widths. Measure each cell's
  content with `measure()` / `text_cell()`; the table width for a card
  spanning the page is `page_width(window) - 2.0`.
- `Metric::new(..).icon(Icon::Send).tint(Tint::Blue).sub(..).on_click(..)`,
  `metric_strip(vec![..])`, `money(amount, size, color)`,
  `money_counting(id, ..)`, `badge`, `status_badge`, `billing_chip`,
  `count_badge`, `client_avatar(name, size)`, `EmptyState::new(..)`,
  `Mascot::new(id, Mood::Happy, size).motion(Motion::Bob)`,
  `stopwatch_dial`, toasts `toast/toast_success/toast_error`.
- Illustrated icons (sidebar/stat art): `cx.global_mut::<Gallery>()
  .get(Art::Icon(Icon::Hourglass), size)` then `img(..)`. Lucide icons:
  `icon(Lucide::Plus).size(px(14.)).text_color(..)` (also `icon_stroke`,
  `icon_filled`). Lucide set is in `icons.rs`; ask if one is missing.
- Shared dialogs as views with events (open with `cx.new`, keep
  `Option<Entity<..>>`, `.children(self.modal.clone())`, subscribe and clear
  it on the event): `ClientFormModal`/`ClientFormEvent`,
  `MergeClientsModal`/`MergeEvent`, `ProjectFormModal`/`ProjectFormEvent`,
  `EntryEditorModal`/`EntryEditorEvent`, `RecordPaymentModal`/
  `RecordPaymentEvent`.

## App plumbing

- A screen implements `Page` (`header()` returns `Header::new(title)
  .subtitle(..).back(label, Route)  .meta(el).actions(el)`, which fills the
  window toolbar like PageHeader.tsx) and `Render` for its body.
- Data: read with `store::db(cx)` (synchronous SQLite). Reload when
  `store::rev(cx)` changes (see Dashboard). Write with
  `store::write(cx, |db| db.something(..))`: on success every screen reloads.
- Navigate with `navigate(Route::Project(id), window, cx)`; routes are in
  `shell/route.rs` (paths mirror the React routes).
- Timer: `crate::timer::{start, pause, resume, stop, toggle_pause}`;
  `store::store(cx).timer_entry()` / `timer_running()` / `timer_paused()`.
- Formatting: `crate::format` (`money`, `duration_short`, `hours_short`,
  `relative`, `relative_days`, `format_date`, `format_day`, `format_time`,
  `money_compact`, `parse_time`, `local_day`, ...). Add what's missing to your
  own file.

## References

`pages/dashboard.rs` (layout, animation, data), `pages/clients.rs` (table,
row menus, modals, search, confirm) and `components/client_form.rs` (a form
modal view) show the patterns.
