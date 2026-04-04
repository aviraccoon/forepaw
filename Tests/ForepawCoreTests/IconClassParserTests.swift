import Testing

@testable import ForepawCore

struct IconClassParserTests {
    let parser = IconClassParser()

    // MARK: - Lucide Icons (Obsidian)

    @Test func lucideSettings() {
        let result = parser.parse(["svg-icon", "lucide-settings"])
        #expect(result == "settings")
    }

    @Test func lucideSearch() {
        let result = parser.parse(["svg-icon", "lucide-search"])
        #expect(result == "search")
    }

    @Test func lucideTerminal() {
        let result = parser.parse(["svg-icon", "lucide-terminal"])
        #expect(result == "terminal")
    }

    @Test func lucideMultiWord() {
        let result = parser.parse(["svg-icon", "lucide-file-search"])
        #expect(result == "file search")
    }

    @Test func lucideLayoutDashboard() {
        let result = parser.parse(["svg-icon", "lucide-layout-dashboard"])
        #expect(result == "layout dashboard")
    }

    @Test func lucideFolderClosed() {
        let result = parser.parse(["svg-icon", "lucide-folder-closed"])
        #expect(result == "folder closed")
    }

    @Test func lucideChevronUpDown() {
        let result = parser.parse(["svg-icon", "lucide-chevrons-up-down"])
        #expect(result == "chevrons up down")
    }

    // MARK: - Tabler Icons (Bruno)

    @Test func tablerHome() {
        let result = parser.parse(["icon", "icon-tabler", "icon-tabler-home"])
        #expect(result == "home")
    }

    @Test func tablerFolder() {
        let result = parser.parse(["icon", "icon-tabler", "icon-tabler-folder"])
        #expect(result == "folder")
    }

    @Test func tablerDownload() {
        let result = parser.parse(["icon", "icon-tabler", "icon-tabler-download"])
        #expect(result == "download")
    }

    @Test func tablerPlus() {
        let result = parser.parse(["icon", "icon-tabler", "icon-tabler-plus"])
        #expect(result == "plus")
    }

    @Test func tablerWithOutlineVariant() {
        let result = parser.parse([
            "icon", "icon-tabler", "icons-tabler-outline", "icon-tabler-layout-sidebar",
        ])
        #expect(result == "layout sidebar")
    }

    @Test func tablerCategory() {
        let result = parser.parse(["icon", "icon-tabler", "icon-tabler-category"])
        #expect(result == "category")
    }

    @Test func tablerDots() {
        let result = parser.parse(["icon", "icon-tabler", "icon-tabler-dots"])
        #expect(result == "dots")
    }

    @Test func tablerBox() {
        let result = parser.parse(["icon", "icon-tabler", "icon-tabler-box"])
        #expect(result == "box")
    }

    // MARK: - FontAwesome

    @Test func fontAwesomeSearch() {
        let result = parser.parse(["fa", "fa-search"])
        #expect(result == "search")
    }

    @Test func fontAwesomeSolidUser() {
        let result = parser.parse(["fas", "fa-user"])
        #expect(result == "user")
    }

    @Test func fontAwesomeBrandsGithub() {
        let result = parser.parse(["fab", "fa-github"])
        #expect(result == "github")
    }

    // MARK: - Material Design Icons

    @Test func materialDesignHome() {
        let result = parser.parse(["mdi", "mdi-home"])
        #expect(result == "home")
    }

    @Test func materialDesignAccountCircle() {
        let result = parser.parse(["mdi", "mdi-account-circle"])
        #expect(result == "account circle")
    }

    // MARK: - Codicons (VS Code)

    @Test func codiconGear() {
        let result = parser.parse(["codicon", "codicon-gear"])
        #expect(result == "gear")
    }

    @Test func codiconSourceControl() {
        let result = parser.parse(["codicon", "codicon-source-control"])
        #expect(result == "source control")
    }

    // MARK: - Bootstrap Icons

    @Test func bootstrapSearch() {
        let result = parser.parse(["bi", "bi-search"])
        #expect(result == "search")
    }

    @Test func bootstrapGear() {
        let result = parser.parse(["bi", "bi-gear-fill"])
        #expect(result == "gear fill")
    }

    // MARK: - Heroicons

    @Test func heroiconHome() {
        let result = parser.parse(["hero-home-solid"])
        #expect(result == "home solid")
    }

    // MARK: - Octicons (GitHub)

    @Test func octiconRepo() {
        let result = parser.parse(["octicon", "octicon-repo"])
        #expect(result == "repo")
    }

    // MARK: - Edge cases

    @Test func emptyClassList() {
        let result = parser.parse([])
        #expect(result == nil)
    }

    @Test func onlyGenericClasses() {
        let result = parser.parse(["icon", "svg-icon"])
        #expect(result == nil)
    }

    @Test func utilityClassesOnly() {
        let result = parser.parse(["flex-shrink-0", "p-1"])
        #expect(result == nil)
    }

    @Test func noRecognizedPrefix() {
        let result = parser.parse(["custom-widget", "my-component"])
        #expect(result == nil)
    }

    @Test func hashedClassNames() {
        // Discord uses hashed class names like "canvas_eb6eba"
        let result = parser.parse(["canvas_eb6eba"])
        #expect(result == nil)
    }

    @Test func mixedWithUtilityClasses() {
        // Bruno pattern: icon classes mixed with styled-components
        let result = parser.parse([
            "StyledWrapper-kbXinc", "eoencc", "action-icon", "p-1",
        ])
        #expect(result == nil)
    }

    @Test func firstIconClassWins() {
        // If multiple icon classes present, first one wins
        let result = parser.parse(["lucide-home", "lucide-settings"])
        #expect(result == "home")
    }

    @Test func obsidianHelpClass() {
        // Obsidian uses bare "help" class on some icons
        let result = parser.parse(["svg-icon", "help"])
        #expect(result == nil)  // "help" alone isn't an icon prefix match
    }

    @Test func realObsidianSidebarToggle() {
        let result = parser.parse(["svg-icon", "lucide-panel-left"])
        #expect(result == "panel left")
    }

    @Test func specialTabIcon() {
        // Bruno uses this for generic tab icons -- not semantic
        let result = parser.parse(["special-tab-icon", "flex-shrink-0"])
        #expect(result == nil)
    }

    @Test func remixIcon() {
        let result = parser.parse(["ri-home-line"])
        #expect(result == "home line")
    }

    @Test func phosphorIcon() {
        let result = parser.parse(["ph-gear-six"])
        #expect(result == "gear six")
    }

    @Test func featherIcon() {
        let result = parser.parse(["feather-arrow-left"])
        #expect(result == "arrow left")
    }
}
