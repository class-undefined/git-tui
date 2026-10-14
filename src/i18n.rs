//! Compile-time checked English/Chinese UI text. Repository content stays verbatim.
use clap::ValueEnum;
use git_tui::git::{
    HeadState, Mutation, Preview, PreviewKind, Snapshot, display_bytes, display_path,
};
use std::os::unix::ffi::OsStrExt;

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, ValueEnum)]
pub(crate) enum Language {
    #[default]
    En,
    Zh,
}

impl Language {
    pub(crate) fn code(self) -> &'static str {
        match self {
            Self::En => "en",
            Self::Zh => "zh",
        }
    }

    pub(crate) fn name(self) -> &'static str {
        match self {
            Self::En => "English",
            Self::Zh => "中文",
        }
    }

    pub(crate) fn parse(value: &str) -> Option<Self> {
        match value {
            "en" => Some(Self::En),
            "zh" => Some(Self::Zh),
            _ => None,
        }
    }

    pub(crate) fn from_locale(value: &str) -> Self {
        if value
            .split(['_', '-', '.', '@'])
            .next()
            .is_some_and(|v| v.eq_ignore_ascii_case("zh"))
        {
            Self::Zh
        } else {
            Self::En
        }
    }
}

// A key cannot exist without both translations; adding a language requires
// updating the exhaustive match below. No string replacement of Git/user data.
macro_rules! translations {
    ($($key:ident => ($en:literal, $zh:literal)),+ $(,)?) => {
        #[derive(Clone, Copy, Debug)]
        pub(crate) enum Text { $($key),+ }
        impl Language {
            pub(crate) fn text(self, key: Text) -> &'static str {
                let pair = match key { $(Text::$key => ($en, $zh)),+ };
                match self { Self::En => pair.0, Self::Zh => pair.1 }
            }
        }
    };
}

translations! {
    SmallTerminal => ("git-tui\nTerminal too small: need 60 columns × 16 rows.\nResize, or Esc then q to quit.", "git-tui\n终端过小：至少需要 60 列 × 16 行。\n请调整窗口，或按 Esc 后按 q 退出。"),
    Working => ("  •  working…", "  •  处理中…"),
    FilesTab => ("1 Files", "1 文件"),
    BranchesTab => ("2 Branches", "2 分支"),
    LogTab => ("3 Log", "3 日志"),
    StageButton => ("s Stage", "s 暂存"),
    UnstageButton => ("u Unstage", "u 取消暂存"),
    CommitButton => ("c Commit", "c 提交"),
    DiffButton => ("d Diff", "d 差异"),
    SwitchButton => ("Enter Switch", "Enter 切换"),
    NewButton => ("n New", "n 新建"),
    RefreshButton => ("r Refresh", "r 刷新"),
    LanguageButton => ("F2 Language", "F2 语言"),
    HelpButton => ("? Help", "? 帮助"),
    QuitButton => ("q Quit", "q 退出"),
    Files => ("Files", "文件"),
    IndexLegend => ("X=index Y=worktree", "X=暂存 Y=工作区"),
    Branches => ("Local branches", "本地分支"),
    History => ("History", "提交历史"),
    WorktreeDiff => (" Worktree diff · d: staged ", " 工作区差异 · d: 暂存区 "),
    StagedDiff => (" Staged diff · d: worktree ", " 暂存区差异 · d: 工作区 "),
    BranchActions => (" Branch actions ", " 分支操作 "),
    CommitDetails => (" Commit details ", " 提交详情 "),
    Footer => (" Click: select/action  Wheel: scroll  Tab: focus  1/2/3: views  F2: language  ?: help", " 点击: 选择/操作  滚轮: 滚动  Tab: 焦点  1/2/3: 视图  F2: 语言  ?: 帮助"),
    CommitTitle => (" Commit staged changes ", " 提交暂存内容 "),
    BranchTitle => (" Create and switch branch ", " 创建并切换分支 "),
    CheckoutTitle => (" Switch branch ", " 切换分支 "),
    HelpTitle => (" Keyboard & mouse ", " 键盘与鼠标 "),
    LanguageTitle => (" Language / 语言 ", " 语言 / Language "),
    CommitPrompt => ("Commit summary (staged changes only):", "提交摘要（仅提交已暂存内容）："),
    BranchPrompt => ("New branch name:", "新分支名称："),
    InputHint => ("Enter: submit   Esc: cancel   Backspace: delete", "Enter: 提交   Esc: 取消   Backspace: 删除"),
    CheckoutHint => ("Uncommitted changes are kept when Git can carry them safely; conflicting changes block checkout.", "Git 会保留可安全携带的未提交修改；存在冲突时会拒绝切换。"),
    SelectedLanguage => ("Selected language:", "已选语言："),
    LanguageHint => ("1/2 or arrows: select   Enter: save   Esc: cancel", "1/2 或方向键: 选择   Enter: 保存   Esc: 取消"),
    Confirm => ("Confirm", "确认"),
    Cancel => ("Cancel", "取消"),
    Close => ("Close", "关闭"),
    LoadingRepository => ("Loading repository…", "正在加载仓库…"),
    Welcome => ("Click a file or use ↑/↓. Press ? for help.", "点击文件或使用 ↑/↓ 选择，按 ? 查看帮助。"),
    LoadingPreview => ("Loading preview…", "正在加载预览…"),
    CleanWorktree => ("Working tree clean.\n\nChanges appear here automatically; press r to refresh.", "工作区干净。\n\n修改会自动显示在这里，也可按 r 刷新。"),
    BranchGuide => ("Local branches\n\nSelect a branch and choose Switch (Enter).\nChoose New (n) to create and switch to a branch.\n\nGit protects uncommitted changes during checkout.", "本地分支\n\n选中分支后选择“切换”（Enter）。\n选择“新建”（n）创建并切换到新分支。\n\n切换时 Git 会保护未提交的修改。"),
    NoCommits => ("No commits yet. Stage a file and create your first commit.", "还没有提交。暂存文件后即可创建首次提交。"),
    RefreshRetry => ("Repository could not be refreshed. Press r to retry.", "仓库刷新失败，按 r 重试。"),
    PreviewFailed => ("Preview failed:", "预览失败："),
    RefreshFailed => ("Refresh failed:", "刷新失败："),
    GitFailed => ("Git operation failed:", "Git 操作失败："),
    AlreadyRunning => ("A Git operation is already running.", "已有 Git 操作正在执行。"),
    RunningGit => ("Running Git operation…", "正在执行 Git 操作…"),
    NoStagedChanges => ("This file has no staged changes.", "此文件没有已暂存的修改。"),
    CurrentBranch => ("Already on this branch.", "已经位于此分支。"),
    ValueRequired => ("Please enter a value.", "请输入内容。"),
    WaitBeforeQuit => ("Waiting for the operation to finish; then press q.", "请等待操作完成后再按 q 退出。"),
    SavingLanguage => ("Saving language preference…", "正在保存语言设置…"),
    LanguageSaved => ("Language saved:", "语言设置已保存："),
    LanguageSaveFailed => ("Language changed for this session; could not save:", "本次会话已切换语言，但无法保存设置："),
    ConfigLoadFailed => ("Could not load language setting; using CLI/system language:", "无法读取语言设置，已使用命令行或系统语言："),
    StartupFailed => ("Could not start git-tui:", "无法启动 git-tui："),
    NeedTerminal => ("git-tui needs an interactive terminal; use --check for diagnostics", "git-tui 需要交互式终端；可使用 --check 查看诊断信息"),
    Repository => ("Repository:", "仓库："),
    ChangedFiles => ("Changed files:", "已修改文件："),
    RecentCommits => ("Recent commits:", "最近提交："),
    Unborn => ("no commits", "尚无提交"),
    Detached => ("detached", "分离头指针"),
    UntrackedFile => ("Untracked file", "未跟踪文件"),
    UntrackedBinary => ("Untracked binary file (preview omitted)", "未跟踪的二进制文件（省略预览）"),
    UntrackedSymlink => ("Untracked symlink", "未跟踪的符号链接"),
    EmptyDiff => ("No changes in this view. Press d to switch worktree / staged diff.", "当前视图没有差异，按 d 切换工作区 / 暂存区。"),
    TruncatedFile => ("[preview truncated at 256 KiB]", "[预览已截断至 256 KiB]"),
    TruncatedDiff => ("[preview truncated at 2 MiB]", "[预览已截断至 2 MiB]"),
    Staged => ("Staged", "已暂存"),
    Unstaged => ("Unstaged", "已取消暂存"),
    WorktreeKept => ("worktree kept", "工作区内容已保留"),
    CommitCreated => ("Commit created", "提交已创建"),
    Switched => ("Switched to", "已切换到"),
    BranchCreated => ("Created branch", "已创建分支"),
    Help => ("1 / 2 / 3     Files / branches / history\nj / k or ↑/↓  Move selection / scroll preview\nTab or h / l  Focus list / preview\nH / L         Scroll preview horizontally\nPgUp / PgDn   Move ten rows\ns or Space    Stage selected file\nu             Unstage (keep worktree)\nd             Worktree / staged diff\nc             Commit staged changes\nn             Create and switch branch\nEnter         Switch branch / focus preview\nr             Refresh (also every 3 seconds)\nF2            Language / 语言\nq / Ctrl-C    Quit when no write is running\n\nMouse: click tabs, rows and buttons; wheel over\na pane scrolls it. Esc closes dialogs.", "1 / 2 / 3     文件 / 分支 / 提交历史\nj / k 或 ↑/↓  移动选择 / 滚动预览\nTab 或 h / l  聚焦列表 / 预览\nH / L         水平滚动预览\nPgUp / PgDn   移动十行\ns 或空格      暂存选中文件\nu             取消暂存（保留工作区）\nd             切换工作区 / 暂存区差异\nc             提交已暂存的修改\nn             创建并切换分支\nEnter         切换分支 / 聚焦预览\nr             刷新（也会每 3 秒自动刷新）\nF2            语言 / Language\nq / Ctrl-C    没有写入操作时退出\n\n鼠标：点击标签、列表行和按钮；滚轮滚动\n指针所在面板。Esc 关闭弹窗。")
}

pub(crate) fn head(snapshot: &Snapshot, language: Language) -> String {
    match snapshot.head_state {
        HeadState::Branch => snapshot.head.clone(),
        HeadState::Unborn => format!("{} ({})", snapshot.head, language.text(Text::Unborn)),
        HeadState::Detached => format!("{} {}", language.text(Text::Detached), snapshot.head),
    }
}

pub(crate) enum Message {
    Text(Text),
    Detail(Text, String),
    Mutation(Mutation),
    RefreshAfter(Box<Message>, String),
}

impl From<Text> for Message {
    fn from(text: Text) -> Self {
        Self::Text(text)
    }
}

impl Message {
    pub(crate) fn render(&self, language: Language) -> String {
        match self {
            Self::Text(key) => language.text(*key).to_owned(),
            Self::Detail(key, detail) => format!("{} {detail}", language.text(*key)),
            Self::RefreshAfter(previous, detail) => format!(
                "{}; {} {detail}",
                previous.render(language),
                language.text(Text::RefreshFailed)
            ),
            Self::Mutation(mutation) => match mutation {
                Mutation::Stage(file) => format!(
                    "{} {}",
                    language.text(Text::Staged),
                    display_path(&file.path)
                ),
                Mutation::Unstage(file) => format!(
                    "{} {} ({})",
                    language.text(Text::Unstaged),
                    display_path(&file.path),
                    language.text(Text::WorktreeKept)
                ),
                Mutation::Commit(_) => language.text(Text::CommitCreated).to_owned(),
                Mutation::Checkout(name) => format!(
                    "{} {}",
                    language.text(Text::Switched),
                    display_bytes(name.as_bytes())
                ),
                Mutation::CreateBranch(name) => format!(
                    "{} {}",
                    language.text(Text::BranchCreated),
                    display_bytes(name.as_bytes())
                ),
            },
        }
    }
}

pub(crate) enum PreviewContent {
    Text(Text),
    Repository(Preview),
    Error(String),
}

impl PreviewContent {
    pub(crate) fn render(&self, language: Language) -> String {
        let preview = match self {
            Self::Text(key) => return language.text(*key).to_owned(),
            Self::Error(error) => {
                return format!("{}\n{error}", language.text(Text::PreviewFailed));
            }
            Self::Repository(preview) => preview,
        };
        let mut body = match preview.kind {
            PreviewKind::Diff if preview.text.is_empty() => {
                language.text(Text::EmptyDiff).to_owned()
            }
            PreviewKind::Diff => preview.text.clone(),
            PreviewKind::UntrackedText => {
                format!("{}\n{}", language.text(Text::UntrackedFile), preview.text)
            }
            PreviewKind::UntrackedBinary => language.text(Text::UntrackedBinary).to_owned(),
            PreviewKind::UntrackedSymlink => format!(
                "{} -> {}",
                language.text(Text::UntrackedSymlink),
                preview.text
            ),
        };
        if preview.truncated {
            body.push('\n');
            body.push_str(language.text(if preview.kind == PreviewKind::Diff {
                Text::TruncatedDiff
            } else {
                Text::TruncatedFile
            }));
        }
        body
    }
}
