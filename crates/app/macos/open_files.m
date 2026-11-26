#import <Cocoa/Cocoa.h>

static void (*g_callback)(const char *) = NULL;

@interface ImageOptimOpenHandler : NSObject
- (void)handleOpen:(NSAppleEventDescriptor *)event withReply:(NSAppleEventDescriptor *)reply;
- (void)handleServices:(NSPasteboard *)pboard userData:(NSString *)userData error:(NSString **)error;
@end

@implementation ImageOptimOpenHandler
- (void)handleOpen:(NSAppleEventDescriptor *)event withReply:(NSAppleEventDescriptor *)reply {
    (void)reply;
    NSAppleEventDescriptor *list = [event paramDescriptorForKeyword:keyDirectObject];
    NSInteger count = [list numberOfItems];
    for (NSInteger i = 1; i <= count; i++) {
        NSAppleEventDescriptor *item = [list descriptorAtIndex:i];
        NSString *value = [item stringValue];
        if (!value) continue;
        NSURL *url = [NSURL URLWithString:value];
        if (!url.isFileURL) continue;
        if (g_callback) g_callback(url.fileSystemRepresentation);
    }
}

- (void)handleServices:(NSPasteboard *)pboard userData:(NSString *)userData error:(NSString **)error {
    (void)userData;
    (void)error;
    NSDictionary *opts = @{NSPasteboardURLReadingFileURLsOnlyKey: @YES};
    NSArray<NSURL *> *urls = [pboard readObjectsForClasses:@[[NSURL class]] options:opts];
    if (urls.count > 0) {
        for (NSURL *url in urls) {
            if (g_callback && url.path.length) g_callback(url.fileSystemRepresentation);
        }
        return;
    }
#pragma clang diagnostic push
#pragma clang diagnostic ignored "-Wdeprecated-declarations"
    NSArray<NSString *> *paths = [pboard propertyListForType:NSFilenamesPboardType];
#pragma clang diagnostic pop
    for (NSString *path in paths) {
        if (g_callback && path.length) g_callback(path.fileSystemRepresentation);
    }
}
@end

// AppleScript `do queuecount command` (ImageOptimVerbs.sdef)
static int g_queue_count = 0;

void imageoptim_set_queue_count(int count) {
    g_queue_count = count;
}

@interface GetQueueCountCommand : NSScriptCommand
@end

@implementation GetQueueCountCommand
- (id)performDefaultImplementation {
    return @(g_queue_count);
}
@end

static void (*g_about_callback)(void) = NULL;

@interface ImageOptimMenuHandler : NSObject
- (void)showAbout:(id)sender;
@end

@implementation ImageOptimMenuHandler
- (void)showAbout:(id)sender {
    (void)sender;
    if (g_about_callback) g_about_callback();
}
@end

static NSMenuItem *findItem(NSMenu *menu, SEL action) {
    for (NSMenuItem *item in menu.itemArray) {
        if (item.action == action) return item;
    }
    return nil;
}

void imageoptim_install_about_handler(void (*callback)(void)) {
    g_about_callback = callback;
    static ImageOptimMenuHandler *handler = nil;
    if (!handler) handler = [ImageOptimMenuHandler new];
    NSMenu *main = [NSApp mainMenu];
    NSMenuItem *appItem = main.itemArray.firstObject;
    NSMenuItem *about = appItem.submenu.itemArray.firstObject;
    if (about) {
        about.target = handler;
        about.action = @selector(showAbout:);
    }
}

void imageoptim_set_app_menu_titles(const char *about, const char *services, const char *hide,
                                    const char *hideOthers, const char *showAll, const char *quit) {
    NSMenu *main = [NSApp mainMenu];
    NSMenuItem *appItem = main.itemArray.firstObject;
    if (!appItem) return;
    if (about) appItem.submenu.itemArray.firstObject.title = [NSString stringWithUTF8String:about];
    NSMenuItem *servicesItem = findItem(appItem.submenu, @selector(submenuAction:));
    if (servicesItem && services) servicesItem.title = [NSString stringWithUTF8String:services];
    NSMenuItem *hideItem = findItem(appItem.submenu, @selector(hide:));
    if (hideItem && hide) hideItem.title = [NSString stringWithUTF8String:hide];
    NSMenuItem *hideOthersItem = findItem(appItem.submenu, @selector(hideOtherApplications:));
    if (hideOthersItem && hideOthers) hideOthersItem.title = [NSString stringWithUTF8String:hideOthers];
    NSMenuItem *showAllItem = findItem(appItem.submenu, @selector(unhideAllApplications:));
    if (showAllItem && showAll) showAllItem.title = [NSString stringWithUTF8String:showAll];
    NSMenuItem *quitItem = findItem(appItem.submenu, @selector(terminate:));
    if (quitItem && quit) quitItem.title = [NSString stringWithUTF8String:quit];
}

void imageoptim_install_open_handler(void (*callback)(const char *)) {
    g_callback = callback;
    static ImageOptimOpenHandler *handler = nil;
    if (!handler) handler = [ImageOptimOpenHandler new];
    [[NSAppleEventManager sharedAppleEventManager]
        setEventHandler:handler
            andSelector:@selector(handleOpen:withReply:)
          forEventClass:kCoreEventClass
             andEventID:kAEOpenDocuments];
    [NSApp setServicesProvider:handler];
}

// ---------------------------------------------------------------------------
// Full main menu (mirrors Base.lproj/ImageOptim.xib)
// ---------------------------------------------------------------------------

typedef struct {
    int rows, selected, busy, againable, again_optable, stoppable, revertable;
    int any_done, dataurl_ok, can_undo, can_redo;
    int lossy, zopfli, pngout, oxipng, advpng, pngcrush, jpegoptim, jpegtran, gifsicle;
    int col_original, col_size, col_savings, col_best;
    const char *undo_title;
    const char *redo_title;
} IOMenuState;

static IOMenuState g_state;
static void (*g_menu_callback)(const char *) = NULL;
static NSMutableDictionary<NSString *, NSMenuItem *> *g_items = nil;

@interface ImageOptimMenuTarget : NSObject
- (void)ioAction:(NSMenuItem *)sender;
- (BOOL)validateMenuItem:(NSMenuItem *)item;
@end

@implementation ImageOptimMenuTarget
- (void)ioAction:(NSMenuItem *)sender {
    NSString *ident = sender.representedObject;
    if (g_menu_callback && ident) g_menu_callback(ident.UTF8String);
}
- (BOOL)validateMenuItem:(NSMenuItem *)item {
    NSString *ident = item.representedObject;
    if (!ident) return YES;
    const IOMenuState *s = &g_state;
    if ([ident isEqualToString:@"lossy"] || [ident isEqualToString:@"quality100"] ||
        [ident hasPrefix:@"tool_"] || [ident hasPrefix:@"col_"]) {
        BOOL on = NO;
        if ([ident isEqualToString:@"lossy"]) on = s->lossy;
        else if ([ident isEqualToString:@"quality100"]) on = !s->lossy;
        else if ([ident isEqualToString:@"tool_zopfli"]) on = s->zopfli;
        else if ([ident isEqualToString:@"tool_pngout"]) on = s->pngout;
        else if ([ident isEqualToString:@"tool_oxipng"]) on = s->oxipng;
        else if ([ident isEqualToString:@"tool_advpng"]) on = s->advpng;
        else if ([ident isEqualToString:@"tool_pngcrush"]) on = s->pngcrush;
        else if ([ident isEqualToString:@"tool_jpegoptim"]) on = s->jpegoptim;
        else if ([ident isEqualToString:@"tool_jpegtran"]) on = s->jpegtran;
        else if ([ident isEqualToString:@"tool_gifsicle"]) on = s->gifsicle;
        else if ([ident isEqualToString:@"col_original"]) on = s->col_original;
        else if ([ident isEqualToString:@"col_size"]) on = s->col_size;
        else if ([ident isEqualToString:@"col_savings"]) on = s->col_savings;
        else if ([ident isEqualToString:@"col_best"]) on = s->col_best;
        item.state = on ? NSControlStateValueOn : NSControlStateValueOff;
        return YES;
    }
    if ([ident isEqualToString:@"again"]) return s->againable;
    if ([ident isEqualToString:@"again_opt"]) return s->again_optable;
    if ([ident isEqualToString:@"stop"]) return s->stoppable;
    if ([ident isEqualToString:@"revert"]) return s->revertable;
    if ([ident isEqualToString:@"delete"] || [ident isEqualToString:@"cut"] ||
        [ident isEqualToString:@"copy"] || [ident isEqualToString:@"reveal"] ||
        [ident isEqualToString:@"quicklook"]) return s->selected > 0;
    if ([ident isEqualToString:@"dataurl"]) return s->dataurl_ok;
    if ([ident isEqualToString:@"delete_complete"]) return s->any_done;
    if ([ident isEqualToString:@"select_all"]) return s->rows > 0;
    if ([ident isEqualToString:@"undo"]) return s->can_undo;
    if ([ident isEqualToString:@"redo"]) return s->can_redo;
    return YES;
}
@end

static NSMenuItem *makeItem(NSString *ident, NSString *title, NSString *key,
                            NSEventModifierFlags mask, ImageOptimMenuTarget *target) {
    NSMenuItem *item = [[NSMenuItem alloc] initWithTitle:title
                                                  action:target ? @selector(ioAction:) : NULL
                                           keyEquivalent:key ?: @""];
    if (ident) {
        item.representedObject = ident;
        g_items[ident] = item;
    }
    item.target = target;
    if (key.length) item.keyEquivalentModifierMask = mask;
    return item;
}

void imageoptim_set_menu_state(const IOMenuState *state) {
    g_state = *state;
    if (state->undo_title && g_items[@"undo"])
        g_items[@"undo"].title = [NSString stringWithUTF8String:state->undo_title];
    if (state->redo_title && g_items[@"redo"])
        g_items[@"redo"].title = [NSString stringWithUTF8String:state->redo_title];
}

void imageoptim_install_menu(const char *titles_tsv, void (*callback)(const char *)) {
    g_menu_callback = callback;
    static ImageOptimMenuTarget *target = nil;
    if (!target) target = [ImageOptimMenuTarget new];
    g_items = [NSMutableDictionary new];

    NSMutableDictionary<NSString *, NSString *> *T = [NSMutableDictionary new];
    NSString *tsv = [NSString stringWithUTF8String:titles_tsv];
    for (NSString *line in [tsv componentsSeparatedByString:@"\n"]) {
        NSRange tab = [line rangeOfString:@"\t"];
        if (tab.location == NSNotFound) continue;
        T[[line substringToIndex:tab.location]] = [line substringFromIndex:tab.location + 1];
    }
    NSString *(^t)(NSString *) = ^NSString *(NSString *k) { return T[k] ?: k; };
    const NSEventModifierFlags Cmd = NSEventModifierFlagCommand;
    const NSEventModifierFlags Opt = NSEventModifierFlagOption;
    const NSEventModifierFlags Shift = NSEventModifierFlagShift;

    NSMenu *main = [NSApp mainMenu];

    // App menu: About, —, Preferences… ⌘,, —, Web API…, Services, …
    NSMenu *appMenu = main.itemArray.firstObject.submenu;
    NSUInteger aboutIdx = 0;
    [appMenu insertItem:[NSMenuItem separatorItem] atIndex:aboutIdx + 1];
    [appMenu insertItem:makeItem(@"prefs", t(@"prefs"), @",", Cmd, target) atIndex:aboutIdx + 2];
    NSMenuItem *services = nil;
    for (NSMenuItem *item in appMenu.itemArray)
        if (item.action == @selector(submenuAction:)) { services = item; break; }
    if (services) {
        NSUInteger idx = [appMenu indexOfItem:services];
        // Upstream order: Preferences, —, Web API, Services (no extra separator).
        [appMenu insertItem:makeItem(@"webapi", t(@"webapi"), @"", 0, target) atIndex:idx];
    }

    // File
    NSMenuItem *fileItem = [[NSMenuItem alloc] initWithTitle:t(@"menu_file") action:NULL keyEquivalent:@""];
    NSMenu *file = [[NSMenu alloc] initWithTitle:t(@"menu_file")];
    fileItem.submenu = file;
    [main addItem:fileItem];
    [file addItem:makeItem(@"add", t(@"add"), @"o", Cmd, target)];
    [file addItem:makeItem(@"again", t(@"again"), @"r", Cmd, target)];
    NSMenuItem *againOpt = makeItem(@"again_opt", t(@"again_opt"), @"r", Cmd | Opt, target);
    againOpt.alternate = YES;
    againOpt.hidden = YES;
    [file addItem:againOpt];
    [file addItem:[NSMenuItem separatorItem]];
    NSMenuItem *close = [[NSMenuItem alloc] initWithTitle:t(@"close") action:@selector(performClose:) keyEquivalent:@"w"];
    close.keyEquivalentModifierMask = Cmd;
    [file addItem:close];
    [file addItem:makeItem(@"stop", t(@"stop"), @"s", Cmd, target)];
    [file addItem:makeItem(@"revert", t(@"revert"), @"", 0, target)];
    [file addItem:[NSMenuItem separatorItem]];
    [file addItem:makeItem(@"quicklook", t(@"quicklook"), @"y", Cmd, target)];
    [file addItem:makeItem(@"reveal", t(@"reveal"), @"", 0, target)];

    // Edit
    NSMenuItem *editItem = [[NSMenuItem alloc] initWithTitle:t(@"menu_edit") action:NULL keyEquivalent:@""];
    NSMenu *edit = [[NSMenu alloc] initWithTitle:t(@"menu_edit")];
    editItem.submenu = edit;
    [main addItem:editItem];
    [edit addItem:makeItem(@"undo", t(@"undo"), @"z", Cmd, target)];
    [edit addItem:makeItem(@"redo", t(@"redo"), @"Z", Cmd | Shift, target)];
    [edit addItem:[NSMenuItem separatorItem]];
    [edit addItem:makeItem(@"cut", t(@"cut"), @"x", Cmd, target)];
    [edit addItem:makeItem(@"copy", t(@"copy"), @"c", Cmd, target)];
    [edit addItem:makeItem(@"dataurl", t(@"dataurl"), @"c", Cmd | Opt, target)];
    [edit addItem:makeItem(@"paste", t(@"paste"), @"v", Cmd, target)];
    [edit addItem:makeItem(@"delete", t(@"delete"), @"\b", Cmd, target)];
    NSMenuItem *delDone = makeItem(@"delete_complete", t(@"delete_complete"), @"\b", Cmd | Opt, target);
    delDone.alternate = YES;
    delDone.hidden = YES;
    [edit addItem:delDone];
    [edit addItem:[NSMenuItem separatorItem]];
    [edit addItem:makeItem(@"select_all", t(@"select_all"), @"a", Cmd, target)];

    // Tools (upstream lists exactly these; no SVGO/svgcleaner/Guetzli)
    NSMenuItem *toolsItem = [[NSMenuItem alloc] initWithTitle:t(@"menu_tools") action:NULL keyEquivalent:@""];
    NSMenu *tools = [[NSMenu alloc] initWithTitle:t(@"menu_tools")];
    toolsItem.submenu = tools;
    [main addItem:toolsItem];
    [tools addItem:makeItem(@"quality100", t(@"quality100"), @"", 0, target)];
    [tools addItem:makeItem(@"lossy", t(@"lossy"), @"l", Cmd, target)];
    [tools addItem:[NSMenuItem separatorItem]];
    [tools addItem:makeItem(@"tool_zopfli", @"Zopfli", @"", 0, target)];
    [tools addItem:makeItem(@"tool_pngout", @"PNGOUT", @"", 0, target)];
    [tools addItem:makeItem(@"tool_oxipng", @"OxiPNG", @"", 0, target)];
    [tools addItem:makeItem(@"tool_advpng", @"AdvPNG", @"", 0, target)];
    [tools addItem:makeItem(@"tool_pngcrush", @"PNGCrush", @"", 0, target)];
    [tools addItem:[NSMenuItem separatorItem]];
    [tools addItem:makeItem(@"tool_jpegoptim", @"JPEGOptim", @"", 0, target)];
    [tools addItem:makeItem(@"tool_jpegtran", @"Jpegtran", @"", 0, target)];
    [tools addItem:[NSMenuItem separatorItem]];
    [tools addItem:makeItem(@"tool_gifsicle", @"Gifsicle", @"", 0, target)];

    // Window
    NSMenuItem *windowItem = [[NSMenuItem alloc] initWithTitle:t(@"menu_window") action:NULL keyEquivalent:@""];
    NSMenu *window = [[NSMenu alloc] initWithTitle:t(@"menu_window")];
    windowItem.submenu = window;
    [main addItem:windowItem];
    NSMenuItem *mini = [[NSMenuItem alloc] initWithTitle:t(@"minimize") action:@selector(performMiniaturize:) keyEquivalent:@"m"];
    mini.keyEquivalentModifierMask = Cmd;
    [window addItem:mini];
    [window addItem:[[NSMenuItem alloc] initWithTitle:t(@"zoom") action:@selector(performZoom:) keyEquivalent:@""]];
    [window addItem:[NSMenuItem separatorItem]];
    [window addItem:[[NSMenuItem alloc] initWithTitle:t(@"front") action:@selector(arrangeInFront:) keyEquivalent:@""]];
    [window addItem:[NSMenuItem separatorItem]];
    NSMenuItem *columnsItem = [[NSMenuItem alloc] initWithTitle:t(@"menu_columns") action:NULL keyEquivalent:@""];
    NSMenu *columns = [[NSMenu alloc] initWithTitle:t(@"menu_columns")];
    columnsItem.submenu = columns;
    [window addItem:columnsItem];
    [columns addItem:makeItem(@"col_original", t(@"col_original"), @"", 0, target)];
    [columns addItem:makeItem(@"col_size", t(@"col_size"), @"", 0, target)];
    [columns addItem:makeItem(@"col_savings", t(@"col_savings"), @"", 0, target)];
    [columns addItem:makeItem(@"col_best", t(@"col_best"), @"", 0, target)];
    [NSApp setWindowsMenu:window];

    // Help
    NSMenuItem *helpItem = [[NSMenuItem alloc] initWithTitle:t(@"menu_help") action:NULL keyEquivalent:@""];
    NSMenu *help = [[NSMenu alloc] initWithTitle:t(@"menu_help")];
    helpItem.submenu = help;
    [main addItem:helpItem];
    [help addItem:makeItem(@"help", t(@"help"), @"", 0, target)];
    [help addItem:makeItem(@"website", t(@"website"), @"", 0, target)];
    [help addItem:makeItem(@"donate", t(@"donate"), @"", 0, target)];
    [help addItem:[NSMenuItem separatorItem]];
    [help addItem:makeItem(@"source", t(@"source"), @"", 0, target)];
    [NSApp setHelpMenu:help];
}

// ---------------------------------------------------------------------------
// Quick Look panel (upstream uses QLPreviewPanel over the selected rows)
// ---------------------------------------------------------------------------

#import <Quartz/Quartz.h>

@interface IOQLItem : NSObject <QLPreviewItem>
@property (nonatomic, strong) NSURL *url;
@property (nonatomic, strong) NSString *name;
@end

@implementation IOQLItem
- (NSURL *)previewItemURL { return self.url; }
- (NSString *)previewItemTitle { return self.name; }
@end

@interface IOQLController : NSObject <QLPreviewPanelDataSource, QLPreviewPanelDelegate>
@property (nonatomic, strong) NSArray<IOQLItem *> *items;
@end

@implementation IOQLController
- (NSInteger)numberOfPreviewItemsInPreviewPanel:(QLPreviewPanel *)panel {
    (void)panel;
    return (NSInteger)self.items.count;
}
- (id<QLPreviewItem>)previewPanel:(QLPreviewPanel *)panel previewItemAtIndex:(NSInteger)index {
    (void)panel;
    return self.items[(NSUInteger)index];
}
@end

static IOQLController *g_ql_controller = nil;

static NSMutableArray<IOQLItem *> *buildQLItems(const char *const *paths, const char *const *titles, int count) {
    NSMutableArray<IOQLItem *> *items = [NSMutableArray new];
    for (int i = 0; i < count; i++) {
        IOQLItem *item = [IOQLItem new];
        item.url = [NSURL fileURLWithPath:[NSString stringWithUTF8String:paths[i]]];
        item.name = [NSString stringWithUTF8String:titles[i]];
        [items addObject:item];
    }
    return items;
}

void imageoptim_quicklook_toggle(const char *const *paths, const char *const *titles, int count) {
    if (!g_ql_controller) g_ql_controller = [IOQLController new];
    NSMutableArray<IOQLItem *> *items = buildQLItems(paths, titles, count);
    QLPreviewPanel *panel = [QLPreviewPanel sharedPreviewPanel];
    if (panel.isVisible) {
        BOOL same = items.count == g_ql_controller.items.count;
        if (same) {
            for (NSUInteger i = 0; i < items.count; i++) {
                if (![items[i].url isEqual:g_ql_controller.items[i].url]) { same = NO; break; }
            }
        }
        if (same) {
            [panel orderOut:nil];
        } else {
            g_ql_controller.items = items;
            [panel reloadData];
        }
        return;
    }
    if (items.count == 0) return;
    g_ql_controller.items = items;
    panel.dataSource = g_ql_controller;
    panel.delegate = g_ql_controller;
    [panel makeKeyAndOrderFront:nil];
}

void imageoptim_quicklook_update_if_visible(const char *const *paths, const char *const *titles, int count) {
    if (![QLPreviewPanel sharedPreviewPanelExists]) return;
    QLPreviewPanel *panel = [QLPreviewPanel sharedPreviewPanel];
    if (!panel.isVisible || count <= 0) return;
    if (!g_ql_controller) g_ql_controller = [IOQLController new];
    g_ql_controller.items = buildQLItems(paths, titles, count);
    panel.dataSource = g_ql_controller;
    panel.delegate = g_ql_controller;
    [panel reloadData];
}

void imageoptim_copy_files(const char *const *paths, int count) {
    if (count <= 0) return;
    NSMutableArray<NSString *> *list = [NSMutableArray arrayWithCapacity:(NSUInteger)count];
    for (int i = 0; i < count; i++) {
        NSString *p = [NSString stringWithUTF8String:paths[i]];
        if (p.length) [list addObject:p];
    }
    if (list.count == 0) return;
    NSPasteboard *pb = [NSPasteboard generalPasteboard];
#pragma clang diagnostic push
#pragma clang diagnostic ignored "-Wdeprecated-declarations"
    [pb declareTypes:@[NSFilenamesPboardType, NSPasteboardTypeString] owner:nil];
    [pb setPropertyList:list forType:NSFilenamesPboardType];
#pragma clang diagnostic pop
    [pb setString:[list componentsJoinedByString:@"\n"] forType:NSPasteboardTypeString];
}

char *imageoptim_paste_paths(void) {
    NSPasteboard *pb = [NSPasteboard generalPasteboard];
    NSDictionary *opts = @{NSPasteboardURLReadingFileURLsOnlyKey: @YES};
    NSArray<NSURL *> *urls = [pb readObjectsForClasses:@[[NSURL class]] options:opts];
    if (urls.count > 0) {
        NSMutableArray<NSString *> *out = [NSMutableArray arrayWithCapacity:urls.count];
        for (NSURL *url in urls) {
            if (url.path.length) [out addObject:url.path];
        }
        if (out.count > 0) {
            return strdup([[out componentsJoinedByString:@"\n"] UTF8String]);
        }
    }
    NSString *text = [pb stringForType:NSPasteboardTypeString];
    if (text.length > 0) {
        return strdup(text.UTF8String);
    }
    return NULL;
}

char *imageoptim_browse_files(const char *exts_csv) {
    NSOpenPanel *panel = [NSOpenPanel openPanel];
    panel.allowsMultipleSelection = YES;
    panel.canChooseFiles = YES;
    panel.canChooseDirectories = YES;
    panel.resolvesAliases = YES;
    if (exts_csv && exts_csv[0]) {
        NSString *csv = [NSString stringWithUTF8String:exts_csv];
        NSArray<NSString *> *types = [csv componentsSeparatedByString:@","];
#pragma clang diagnostic push
#pragma clang diagnostic ignored "-Wdeprecated-declarations"
        panel.allowedFileTypes = types;
#pragma clang diagnostic pop
    }
    if ([panel runModal] != NSModalResponseOK) return NULL;
    NSMutableArray<NSString *> *out = [NSMutableArray new];
    for (NSURL *url in panel.URLs) {
        if (url.path.length) [out addObject:url.path];
    }
    if (out.count == 0) return NULL;
    return strdup([[out componentsJoinedByString:@"\n"] UTF8String]);
}

void imageoptim_free_string(char *s) {
    free(s);
}

bool imageoptim_app_is_active(void) {
    return [NSApp isActive];
}

bool imageoptim_trash_file(const char *path, char *out, int out_len) {
    NSURL *url = [NSURL fileURLWithPath:[NSString stringWithUTF8String:path]];
    NSURL *result = nil;
    NSError *err = nil;
    if ([[NSFileManager defaultManager] trashItemAtURL:url resultingItemURL:&result error:&err] && result) {
        const char *fs = result.path.fileSystemRepresentation;
        if ((int)strlen(fs) >= out_len) return false;
        strcpy(out, fs);
        return true;
    }
    return false;
}

void imageoptim_beep(void) {
    NSBeep();
}

void imageoptim_reveal_file(const char *path) {
    NSURL *url = [NSURL fileURLWithPath:[NSString stringWithUTF8String:path]];
    [[NSWorkspace sharedWorkspace] activateFileViewerSelectingURLs:@[url]];
}

// ---------------------------------------------------------------------------
// External file drag-out (tableView:writeRowsWithIndexes:toPasteboard:)
// ---------------------------------------------------------------------------

@interface IODragSource : NSObject <NSDraggingSource>
@end

@implementation IODragSource
- (NSDragOperation)draggingSession:(NSDraggingSession *)session
    sourceOperationMaskForDraggingContext:(NSDraggingContext)context {
    (void)session;
    (void)context;
    return NSDragOperationCopy;
}
@end

bool imageoptim_begin_file_drag(const char *const *paths, int count) {
    if (count <= 0) return false;
    NSWindow *win = [NSApp keyWindow] ?: [NSApp mainWindow];
    if (!win || !win.contentView) return false;

    static IODragSource *source = nil;
    if (!source) source = [IODragSource new];

    NSMutableArray<NSDraggingItem *> *items = [NSMutableArray arrayWithCapacity:(NSUInteger)count];
    NSPoint mouseInScreen = [NSEvent mouseLocation];
    NSPoint mouseInWindow = [win convertPointFromScreen:mouseInScreen];
    NSPoint mouseInView = [win.contentView convertPoint:mouseInWindow fromView:nil];

    for (int i = 0; i < count; i++) {
        NSString *path = [NSString stringWithUTF8String:paths[i]];
        if (!path.length) continue;
        NSURL *url = [NSURL fileURLWithPath:path];
        NSDraggingItem *item = [[NSDraggingItem alloc] initWithPasteboardWriter:url];
        NSImage *icon = [[NSWorkspace sharedWorkspace] iconForFile:path];
        icon.size = NSMakeSize(32, 32);
        int offset = (i < 5 ? i : 5) * 4;
        NSRect dragFrame = NSMakeRect(mouseInView.x - 16 + offset, mouseInView.y - 16 - offset, 32, 32);
        [item setDraggingFrame:dragFrame contents:icon];
        [items addObject:item];
    }
    if (items.count == 0) return false;

    NSEvent *cur = [NSApp currentEvent];
    NSEvent *dragEvent = nil;
    if (cur && (cur.type == NSEventTypeLeftMouseDragged || cur.type == NSEventTypeLeftMouseDown)) {
        dragEvent = cur;
    } else {
        dragEvent = [NSEvent mouseEventWithType:NSEventTypeLeftMouseDragged
                                       location:mouseInWindow
                                  modifierFlags:0
                                      timestamp:[[NSProcessInfo processInfo] systemUptime]
                                   windowNumber:win.windowNumber
                                        context:nil
                                    eventNumber:0
                                     clickCount:1
                                       pressure:1.0];
    }
    if (!dragEvent) return false;
    [win.contentView beginDraggingSessionWithItems:items event:dragEvent source:source];
    return true;
}
