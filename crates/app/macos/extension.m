// macOS Action / Markup UI Service Extension (`ImageOptimize.appex`).
//
// Improvements over upstream ImageOptim 1.9.3 `ExtensionController`:
// 1. Handles `NSURL`, `NSData`, and `loadDataRepresentationForTypeIdentifier:`
//    instead of failing with "Received something that's not NSData" when Finder
//    Quick Actions pass file URLs.
// 2. Supports batch optimization (up to 100 images) instead of a single image,
//    plus SVG (`public.svg-image`) alongside PNG, JPEG, and GIF.
// 3. Built programmatically without nib dependencies and localized in English
//    and Simplified Chinese (`zh-Hans`).

#import <Cocoa/Cocoa.h>
#include <pwd.h>
#include <unistd.h>

static NSString *io_ext_shared_settings_path(void) {
    NSFileManager *fm = [NSFileManager defaultManager];
    NSURL *groupURL =
        [fm containerURLForSecurityApplicationGroupIdentifier:@"group.io.github.hex-ci.imageoptim-rust"];
    if (groupURL) {
        NSString *gp = [groupURL.path stringByAppendingPathComponent:@"settings.json"];
        if ([fm isReadableFileAtPath:gp]) {
            return gp;
        }
    }
    struct passwd *pw = getpwuid(getuid());
    NSString *realHome = (pw && pw->pw_dir) ? [NSString stringWithUTF8String:pw->pw_dir]
                                            : NSHomeDirectory();
    NSString *appSupport = [[realHome
        stringByAppendingPathComponent:@"Library/Application Support/io.github.hex-ci.ImageOptim-Rust"]
        stringByAppendingPathComponent:@"settings.json"];
    return [fm isReadableFileAtPath:appSupport] ? appSupport : nil;
}

static BOOL io_ext_is_zh(void) {
    NSArray<NSString *> *langs = [NSLocale preferredLanguages];
    NSString *first = langs.firstObject ?: @"en";
    return [first hasPrefix:@"zh"];
}

static NSString *io_ext_pick_type(NSItemProvider *provider) {
    NSArray<NSString *> *preferred = @[
        @"public.png",
        @"public.jpeg",
        @"com.compuserve.gif",
        @"public.svg-image",
        @"public.image",
    ];
    for (NSString *uti in preferred) {
        if ([provider hasItemConformingToTypeIdentifier:uti]) {
            return uti;
        }
    }
    return nil;
}

static NSString *io_ext_extension_for_uti(NSString *uti) {
    if ([uti isEqualToString:@"public.png"]) return @"png";
    if ([uti isEqualToString:@"public.jpeg"]) return @"jpg";
    if ([uti isEqualToString:@"com.compuserve.gif"]) return @"gif";
    if ([uti isEqualToString:@"public.svg-image"]) return @"svg";
    return @"img";
}

static NSString *io_ext_detect_uti_from_bytes(NSData *data, NSString *fallback) {
    const unsigned char *b = (const unsigned char *)data.bytes;
    NSUInteger n = data.length;
    if (n >= 8 && b[0] == 0x89 && b[1] == 'P' && b[2] == 'N' && b[3] == 'G') {
        return @"public.png";
    }
    if (n >= 3 && b[0] == 0xFF && b[1] == 0xD8 && b[2] == 0xFF) {
        return @"public.jpeg";
    }
    if (n >= 6 && memcmp(b, "GIF8", 4) == 0) {
        return @"com.compuserve.gif";
    }
    if ([fallback isEqualToString:@"public.image"]) {
        return @"public.png";
    }
    return fallback ?: @"public.png";
}

@interface ExtensionController : NSViewController
@end

@implementation ExtensionController {
    NSTextField *_statusLabel;
    NSProgressIndicator *_spinner;
    NSTask *_activeTask;
    NSLock *_taskLock;
    BOOL _cancelled;
    BOOL _stopped;
}

- (void)loadView {
    _taskLock = [[NSLock alloc] init];
    BOOL zh = io_ext_is_zh();

    NSView *root = [[NSView alloc] initWithFrame:NSMakeRect(0, 0, 300, 96)];

    _spinner = [[NSProgressIndicator alloc] initWithFrame:NSMakeRect(20, 52, 20, 20)];
    _spinner.style = NSProgressIndicatorStyleSpinning;
    _spinner.controlSize = NSControlSizeSmall;
    [_spinner startAnimation:nil];
    [root addSubview:_spinner];

    _statusLabel = [NSTextField labelWithString:zh ? @"正在使用 ImageOptim-Rust 优化…"
                                                   : @"Optimizing with ImageOptim-Rust…"];
    _statusLabel.frame = NSMakeRect(48, 52, 232, 20);
    _statusLabel.font = [NSFont systemFontOfSize:12.0];
    _statusLabel.lineBreakMode = NSLineBreakByTruncatingTail;
    [root addSubview:_statusLabel];

    NSButton *stopBtn = [NSButton buttonWithTitle:(zh ? @"完成" : @"Stop")
                                           target:self
                                           action:@selector(stop:)];
    stopBtn.frame = NSMakeRect(126, 14, 76, 28);
    [root addSubview:stopBtn];

    NSButton *cancelBtn = [NSButton buttonWithTitle:(zh ? @"取消" : @"Cancel")
                                             target:self
                                             action:@selector(cancel:)];
    cancelBtn.frame = NSMakeRect(208, 14, 76, 28);
    cancelBtn.keyEquivalent = @"\x1b";
    [root addSubview:cancelBtn];

    self.view = root;
}

- (void)viewDidAppear {
    [super viewDidAppear];
    static dispatch_once_t once;
    // Per-instance guard in case viewDidAppear fires more than once.
    if (_spinner.isHidden) {
        (void)once;
        return;
    }
    [self startOptimization];
}

- (NSURL *)hostExecutableURL {
    // Bundle path: ImageOptim-Rust.app/Contents/PlugIns/ImageOptimize.appex
    NSURL *appexURL = [NSBundle mainBundle].bundleURL;
    NSURL *contentsURL = [[appexURL URLByDeletingLastPathComponent] URLByDeletingLastPathComponent];
    return [[contentsURL URLByAppendingPathComponent:@"MacOS" isDirectory:YES]
        URLByAppendingPathComponent:@"ImageOptim-Rust"
                        isDirectory:NO];
}

- (void)loadProviderData:(NSItemProvider *)provider
                    type:(NSString *)uti
              completion:(void (^)(NSData *_Nullable))completion {
    [provider loadDataRepresentationForTypeIdentifier:uti
                                    completionHandler:^(NSData *data, NSError *err) {
        if (data.length > 0) {
            completion(data);
            return;
        }
        (void)err;
        if ([provider canLoadObjectOfClass:[NSURL class]]) {
            [provider loadObjectOfClass:[NSURL class]
                      completionHandler:^(id<NSItemProviderReading> item, NSError *urlErr) {
                (void)urlErr;
                NSURL *url = (NSURL *)item;
                completion([url isKindOfClass:[NSURL class]] ? [NSData dataWithContentsOfURL:url]
                                                             : nil);
            }];
        } else if ([provider canLoadObjectOfClass:[NSImage class]]) {
            [provider loadObjectOfClass:[NSImage class]
                      completionHandler:^(id<NSItemProviderReading> item, NSError *imgErr) {
                (void)imgErr;
                NSImage *img = (NSImage *)item;
                NSData *tiff = [img isKindOfClass:[NSImage class]] ? [img TIFFRepresentation] : nil;
                NSBitmapImageRep *rep = tiff ? [NSBitmapImageRep imageRepWithData:tiff] : nil;
                NSData *png = [rep representationUsingType:NSBitmapImageFileTypePNG
                                                properties:@{}];
                completion(png);
            }];
        } else {
            completion(nil);
        }
    }];
}

- (void)startOptimization {
    NSExtensionContext *extCtx = self.extensionContext;
    NSArray<NSExtensionItem *> *inputItems = extCtx.inputItems;
    if (inputItems.count == 0) {
        NSError *err = [NSError errorWithDomain:NSCocoaErrorDomain
                                           code:NSUserCancelledError
                                       userInfo:nil];
        [extCtx cancelRequestWithError:err];
        return;
    }

    dispatch_async(dispatch_get_global_queue(QOS_CLASS_USER_INITIATED, 0), ^{
        [self processItemsOnWorker:inputItems context:extCtx];
    });
}

- (void)processItemsOnWorker:(NSArray<NSExtensionItem *> *)inputItems
                     context:(NSExtensionContext *)extCtx {
    BOOL zh = io_ext_is_zh();
    NSURL *cliURL = [self hostExecutableURL];
    NSFileManager *fm = [NSFileManager defaultManager];
    NSMutableArray<NSExtensionItem *> *outputItems = [NSMutableArray array];
    NSUInteger index = 0;

    for (NSExtensionItem *inItem in inputItems) {
        if (_cancelled) break;
        NSMutableArray<NSItemProvider *> *outProviders = [NSMutableArray array];

        for (NSItemProvider *provider in inItem.attachments) {
            if (_cancelled) break;
            NSString *uti = io_ext_pick_type(provider);
            if (!uti) {
                [outProviders addObject:provider];
                continue;
            }

            index += 1;
            dispatch_async(dispatch_get_main_queue(), ^{
                self->_statusLabel.stringValue =
                    zh ? [NSString stringWithFormat:@"正在优化第 %lu 张图像…", (unsigned long)index]
                       : [NSString stringWithFormat:@"Optimizing image %lu…", (unsigned long)index];
            });

            __block NSData *rawData = nil;
            dispatch_semaphore_t sem = dispatch_semaphore_create(0);
            [self loadProviderData:provider
                              type:uti
                        completion:^(NSData *loaded) {
                            rawData = loaded;
                            dispatch_semaphore_signal(sem);
                        }];
            dispatch_semaphore_wait(sem, DISPATCH_TIME_FOREVER);

            if (rawData.length == 0 || _cancelled) {
                [outProviders addObject:provider];
                continue;
            }

            if (!_stopped && [fm isExecutableFileAtPath:cliURL.path]) {
                NSString *ext = io_ext_extension_for_uti(uti);
                NSString *tmpName = [NSString
                    stringWithFormat:@"imageoptim-ext-%@.%@", [NSUUID UUID].UUIDString, ext];
                NSURL *tmpURL = [NSURL
                    fileURLWithPath:[NSTemporaryDirectory() stringByAppendingPathComponent:tmpName]];

                if ([rawData writeToURL:tmpURL atomically:YES]) {
                    NSTask *task = [[NSTask alloc] init];
                    task.executableURL = cliURL;
                    NSMutableArray<NSString *> *args =
                        [NSMutableArray arrayWithObjects:@"--cli", @"--no-backup", nil];
                    NSString *sharedSettings = io_ext_shared_settings_path();
                    if (sharedSettings) {
                        [args addObject:@"--settings"];
                        [args addObject:sharedSettings];
                    }
                    [args addObject:tmpURL.path];
                    task.arguments = args;
                    task.standardOutput = [NSPipe pipe];
                    task.standardError = [NSPipe pipe];

                    [_taskLock lock];
                    _activeTask = task;
                    [_taskLock unlock];

                    NSError *launchErr = nil;
                    if ([task launchAndReturnError:&launchErr]) {
                        [task waitUntilExit];
                    }

                    [_taskLock lock];
                    _activeTask = nil;
                    [_taskLock unlock];

                    if (!_cancelled) {
                        NSData *optData = [NSData dataWithContentsOfURL:tmpURL];
                        if (optData.length > 0 && optData.length <= rawData.length) {
                            rawData = optData;
                        }
                    }
                    [fm removeItemAtURL:tmpURL error:nil];
                }
            }

            NSString *outUTI = io_ext_detect_uti_from_bytes(rawData, uti);
            NSData *finalData = [rawData copy];
            NSItemProvider *outProvider = [[NSItemProvider alloc] init];
            [outProvider registerDataRepresentationForTypeIdentifier:outUTI
                                                          visibility:NSItemProviderRepresentationVisibilityAll
                                                         loadHandler:^NSProgress *(
                                                             void (^handler)(NSData *, NSError *)) {
                handler(finalData, nil);
                return nil;
            }];
            [outProviders addObject:outProvider];
        }

        NSExtensionItem *outItem = [inItem copy];
        outItem.attachments = outProviders;
        [outputItems addObject:outItem];
    }

    dispatch_async(dispatch_get_main_queue(), ^{
        [self->_spinner stopAnimation:nil];
        self->_spinner.hidden = YES;
        if (self->_cancelled) {
            NSError *err = [NSError errorWithDomain:NSCocoaErrorDomain
                                               code:NSUserCancelledError
                                           userInfo:nil];
            [extCtx cancelRequestWithError:err];
        } else {
            [extCtx completeRequestReturningItems:outputItems completionHandler:nil];
        }
    });
}

- (IBAction)stop:(id)sender {
    (void)sender;
    _stopped = YES;
    [_taskLock lock];
    if (_activeTask.isRunning) {
        [_activeTask terminate];
    }
    [_taskLock unlock];
}

- (IBAction)cancel:(id)sender {
    (void)sender;
    _cancelled = YES;
    [_taskLock lock];
    if (_activeTask.isRunning) {
        [_activeTask terminate];
    }
    [_taskLock unlock];
}

@end
