// Small Apple API boundary shared by the Mac client and the iPhone core.
#import <AVFoundation/AVFoundation.h>
#import <ImageIO/ImageIO.h>
#import <TargetConditionals.h>
#include <math.h>
#include <stdint.h>
#include <stdlib.h>
#include <string.h>

// Runs only on an upload preparation worker. The source is a local file, the
// decoder produces at most 96x96 pixels, and async operations have deadlines.
bool whatsapp_video_poster(const char *path, double *seconds, uint32_t *width,
                           uint32_t *height, void **jpeg, size_t *length) {
    @autoreleasepool {
        @try {
            NSString *name = [[NSFileManager defaultManager] stringWithFileSystemRepresentation:path length:strlen(path)];
            if (!name) return false;
            AVURLAsset *asset = [AVURLAsset URLAssetWithURL:[NSURL fileURLWithPath:name] options:nil];
            dispatch_semaphore_t ready = dispatch_semaphore_create(0);
            // Objective-C's property loading API; completion keeps all captured
            // objects alive if the deadline expires. No Rust pointer is captured.
            [asset loadValuesAsynchronouslyForKeys:@[@"duration", @"tracks"] completionHandler:^{
                dispatch_semaphore_signal(ready);
            }];
            if (dispatch_semaphore_wait(ready, dispatch_time(DISPATCH_TIME_NOW, 5 * NSEC_PER_SEC))) {
                [asset cancelLoading];
                return false;
            }
            if ([asset statusOfValueForKey:@"duration" error:nil] != AVKeyValueStatusLoaded ||
                [asset statusOfValueForKey:@"tracks" error:nil] != AVKeyValueStatusLoaded) return false;
            double duration = CMTimeGetSeconds(asset.duration);
            if (!isfinite(duration) || duration <= 0) return false;
            // The tracks key was loaded above. Its Objective-C property also
            // supports macOS 11, without the deprecated synchronous filter API.
            AVAssetTrack *track = nil;
            for (AVAssetTrack *candidate in asset.tracks) {
                if ([candidate.mediaType isEqualToString:AVMediaTypeVideo]) {
                    track = candidate;
                    break;
                }
            }
            if (!track) return false;
            CGRect bounds = CGRectApplyAffineTransform((CGRect){CGPointZero, track.naturalSize}, track.preferredTransform);
            double w = fabs(bounds.size.width), h = fabs(bounds.size.height);
            if (!isfinite(w) || !isfinite(h) || w < 1 || h < 1 || w > UINT32_MAX || h > UINT32_MAX) return false;
            *seconds = duration; *width = (uint32_t)llround(w); *height = (uint32_t)llround(h);
            AVAssetImageGenerator *generator = [AVAssetImageGenerator assetImageGeneratorWithAsset:asset];
            generator.appliesPreferredTrackTransform = YES;
            generator.maximumSize = CGSizeMake(96, 96);
            dispatch_semaphore_t rendered = dispatch_semaphore_create(0);
            __block NSData *result = nil;
            NSValue *time = [NSValue valueWithCMTime:CMTimeMakeWithSeconds(fmin(1, duration / 2), 600)];
            [generator generateCGImagesAsynchronouslyForTimes:@[time]
                completionHandler:^(CMTime requested, CGImageRef image, CMTime actual, AVAssetImageGeneratorResult status, NSError *error) {
                    (void)requested; (void)actual; (void)error;
                    if (status == AVAssetImageGeneratorSucceeded && image) {
                        NSMutableData *data = [NSMutableData data];
                        CGImageDestinationRef output = CGImageDestinationCreateWithData((__bridge CFMutableDataRef)data, CFSTR("public.jpeg"), 1, NULL);
                        if (output) {
                            CGImageDestinationAddImage(output, image, (__bridge CFDictionaryRef)@{(__bridge NSString *)kCGImageDestinationLossyCompressionQuality: @0.7});
                            if (CGImageDestinationFinalize(output)) result = data;
                            CFRelease(output);
                        }
                    }
                    dispatch_semaphore_signal(rendered);
                }];
            if (dispatch_semaphore_wait(rendered, dispatch_time(DISPATCH_TIME_NOW, 5 * NSEC_PER_SEC))) {
                [generator cancelAllCGImageGeneration];
                return true; // dimensions/duration still help if decoding fails
            }
            if (result.length > 0 && result.length <= 128 * 1024) {
                void *copy = malloc(result.length);
                if (copy) {
                    memcpy(copy, result.bytes, result.length);
                    *jpeg = copy; *length = result.length;
                }
            }
            return true;
        } @catch (NSException *exception) {
            return false; // Never allow Objective-C exceptions to cross Rust.
        }
    }
}

void whatsapp_native_free(void *bytes) { free(bytes); }

#if TARGET_OS_OSX
#import <AppKit/AppKit.h>
// Catch inside Objective-C rather than unwinding through Rust's event loop.
// Keep the existing event-driven wake source and cap native work per drain.
static int appkit_boundary(int (^drain)(void)) {
    @autoreleasepool {
        @try {
            return drain();
        } @catch (NSException *exception) { return -1; }
    }
}

int whatsapp_appkit_drain(void) {
    return appkit_boundary(^int {
            int handled = 0;
            NSApplication *app = NSApplication.sharedApplication;
            for (int i = 0; i < 64; i++) {
                NSEvent *event = [app nextEventMatchingMask:NSEventMaskAny untilDate:NSDate.distantPast inMode:NSDefaultRunLoopMode dequeue:YES];
                if (!event) break;
                [app sendEvent:event];
                handled++;
            }
            return handled;
    });
}

#ifdef WHATSAPP_NATIVE_PROBE
// Synthetic exception: exercises the same boundary without touching a window.
bool whatsapp_appkit_exception_probe(void) {
    int failed = appkit_boundary(^int {
        [NSException raise:@"FixtureException" format:@"synthetic"];
        return 0;
    });
    int recovered = appkit_boundary(^int { return 7; });
    return failed == -1 && recovered == 7;
}
#endif
#endif
