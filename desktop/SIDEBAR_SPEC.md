# Kelpie Sidebar Architecture & Specification

## 1. Executive Summary

The Kelpie sidebar is the primary command-and-control rail for triaging a fleet of OMP coding agents across `herdr` workspaces. This specification defines the visual hierarchy, data extraction, Lucide iconography, and grouping principles for the desktop console.

---

## 2. Core Design Principles

1. **Workspace-Anchored Spatial Grouping**
   - Agents run in specific directories and git repositories. Grouping agents under workspace headers (e.g. `olympus`, `misty-step/kelpie`) anchors them in their physical codebase context.

2. **Attention-Sorted Inner Priority**
   - Within each workspace group, panes are sorted by operational urgency:
     - **Needs Input / Pending Ask** (Green badge + `<MessageSquareCode />`)
     - **Blocked** (Red badge + `<AlertOctagon />`)
     - **Working** (Periwinkle spinner + `<Loader2 />`)
     - **Idle / Done** (Muted neutral + `<CheckCircle2 />` / `<Clock />`)

3. **Dense Two-Line Metadata**
   - **Line 1**: Agent Task / Objective + Relative Age (`3m`, `1h`).
   - **Line 2**: Working Directory (`~/.../demo/wA`) + Tail Snippet / Current Action (`Editing windowApi.ts`).

4. **Lucide Iconography Vocabulary**
   - **Workspace**: `<FolderGit2 size={13} />`
   - **Working**: `<Loader2 size={12} className="spin" />`
   - **Needs Input**: `<HelpCircle size={12} />`
   - **Blocked**: `<AlertOctagon size={12} />`
   - **Done**: `<CheckCircle2 size={12} />`
   - **Idle**: `<Clock size={12} />`

5. **Codex-Desktop Visual Hierarchy**
   - `#f3f3f3` sidebar background, 1px `#e0e0e0` borders, `#ffffff` elevated card selection with 1px border.

---

## 3. Data Requirements & Wire Mapping

| Field | Source | Visual Mapping |
|---|---|---|
| Workspace Label | `Workspace.label` | Section Header title |
| Workspace CWD | `Pane.cwd` (grouped) | Section Header sub-path |
| Agent Task Title | `Pane.task` (`taskTitle`) | Row Line 1 bold text |
| Status / Urgency | `Pane.status` + `pending_ask` | Lucide Status Chip + Rank |
| Recency | `Pane.updated_ms` (`relativeTime`) | Row Line 1 timestamp |
| Working Directory | `Pane.cwd` | Row Line 2 path badge |
| Activity Snippet | `Pane.snippet` | Row Line 2 tail preview |

---

## 4. Component Hierarchy

```
Sidebar
 ├── SidebarHeader ("Fleet" count + status indicator)
 ├── WorkspaceSection (per workspace)
 │    ├── WorkspaceHeader (Icon + Label + Path + Count + Collapse Toggle)
 │    └── PaneRow (attention-sorted)
 │         ├── RowTop (Status Icon + Task Title + Time)
 │         └── RowSub (CWD mono badge + Activity Snippet)
 └── SidebarFooter (Settings action)
```
