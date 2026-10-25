// Small Apple API boundary shared by the Mac client and the iPhone core.
#import <AVFoundation/AVFoundation.h>
#import <ImageIO/ImageIO.h>
#import <TargetConditionals.h>
#include <math.h>
#include <stdint.h>
#include <stdlib.h>
#include <string.h>
#include <unistd.h>

// Runs only on bounded background workers. Sources are local files and both
// native loading and frame generation have deadlines.
static bool video_poster(const char *path, double *seconds, uint32_t *width,
                         uint32_t *height, void **jpeg, size_t *length,
                         unsigned side, size_t byte_limit, double quality) {
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
            generator.maximumSize = CGSizeMake(side, side);
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
                            CGImageDestinationAddImage(output, image, (__bridge CFDictionaryRef)@{(__bridge NSString *)kCGImageDestinationLossyCompressionQuality: @(quality)});
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
            if (result.length > 0 && result.length <= byte_limit) {
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

// Preserve the compact thumbnail sent over WhatsApp, including on iOS.
bool whatsapp_video_poster(const char *path, double *seconds, uint32_t *width,
                           uint32_t *height, void **jpeg, size_t *length) {
    return video_poster(path, seconds, width, height, jpeg, length, 96, 128 * 1024, 0.7);
}

void whatsapp_native_free(void *bytes) { free(bytes); }

// Browsers play frame delays of 10 ms or less at 100 ms; so does this.
static int64_t animation_delay_ms(CGImageSourceRef source, size_t index) {
    NSDictionary *properties = CFBridgingRelease(CGImageSourceCopyPropertiesAtIndex(source, index, NULL));
    NSDictionary *gif = properties[(__bridge NSString *)kCGImagePropertyGIFDictionary];
    NSDictionary *webp = properties[(__bridge NSString *)kCGImagePropertyWebPDictionary];
    NSNumber *delay = gif[(__bridge NSString *)kCGImagePropertyGIFUnclampedDelayTime]
        ?: gif[(__bridge NSString *)kCGImagePropertyGIFDelayTime]
        ?: webp[(__bridge NSString *)kCGImagePropertyWebPUnclampedDelayTime]
        ?: webp[(__bridge NSString *)kCGImagePropertyWebPDelayTime];
    double seconds = delay.doubleValue;
    if (!isfinite(seconds) || seconds <= 0.011) return 100;
    return llround(fmin(seconds, 60) * 1000);
}

static bool animation_video(CGImageSourceRef source, NSURL *output) {
    size_t count = CGImageSourceGetCount(source);
    if (count < 2 || count > 1500) return false; // still images keep their own path
    NSDictionary *uncached = @{(__bridge NSString *)kCGImageSourceShouldCache: @NO};
    CGImageRef first = CGImageSourceCreateImageAtIndex(source, 0, (__bridge CFDictionaryRef)uncached);
    if (!first) return false;
    size_t canvas_width = CGImageGetWidth(first), canvas_height = CGImageGetHeight(first);
    CGImageRelease(first);
    if (canvas_width < 1 || canvas_height < 1 || canvas_width > 4096 || canvas_height > 4096 ||
        canvas_width * canvas_height > 8 * 1024 * 1024) return false;
    // WhatsApp's GIF videos are small; H.264 4:2:0 needs even dimensions.
    double scale = fmin(1.0, 640.0 / (double)MAX(canvas_width, canvas_height));
    size_t width = (size_t)(canvas_width * scale) & ~(size_t)1;
    size_t height = (size_t)(canvas_height * scale) & ~(size_t)1;
    if (width < 16 || height < 16) return false;
    NSError *error = nil;
    AVAssetWriter *writer = [AVAssetWriter assetWriterWithURL:output fileType:AVFileTypeMPEG4 error:&error];
    if (!writer) return false;
    writer.shouldOptimizeForNetworkUse = YES;
    long bitrate = lround(fmin(fmax(width * height * 3.0, 250000), 2500000));
    NSDictionary *settings = @{
        AVVideoCodecKey: AVVideoCodecTypeH264,
        AVVideoWidthKey: @(width),
        AVVideoHeightKey: @(height),
        AVVideoColorPropertiesKey: @{
            AVVideoColorPrimariesKey: AVVideoColorPrimaries_ITU_R_709_2,
            AVVideoTransferFunctionKey: AVVideoTransferFunction_ITU_R_709_2,
            AVVideoYCbCrMatrixKey: AVVideoYCbCrMatrix_ITU_R_709_2,
        },
        AVVideoCompressionPropertiesKey: @{
            AVVideoProfileLevelKey: AVVideoProfileLevelH264MainAutoLevel,
            AVVideoAverageBitRateKey: @(bitrate),
            AVVideoAllowFrameReorderingKey: @NO,
        },
    };
    AVAssetWriterInput *input = [AVAssetWriterInput assetWriterInputWithMediaType:AVMediaTypeVideo outputSettings:settings];
    input.expectsMediaDataInRealTime = NO;
    AVAssetWriterInputPixelBufferAdaptor *adaptor = [AVAssetWriterInputPixelBufferAdaptor
        assetWriterInputPixelBufferAdaptorWithAssetWriterInput:input
        sourcePixelBufferAttributes:@{
            (__bridge NSString *)kCVPixelBufferPixelFormatTypeKey: @(kCVPixelFormatType_32BGRA),
            (__bridge NSString *)kCVPixelBufferWidthKey: @(width),
            (__bridge NSString *)kCVPixelBufferHeightKey: @(height),
        }];
    if (![writer canAddInput:input]) return false;
    [writer addInput:input];
    if (![writer startWriting]) return false;
    [writer startSessionAtSourceTime:kCMTimeZero];
    CGColorSpaceRef space = CGColorSpaceCreateWithName(kCGColorSpaceSRGB);
    if (!space) {
        [writer cancelWriting];
        return false;
    }
    dispatch_time_t deadline = dispatch_time(DISPATCH_TIME_NOW, 60 * NSEC_PER_SEC);
    int64_t elapsed = 0;
    bool written = true;
    for (size_t index = 0; index < count && written; index++) {
        @autoreleasepool {
            CGImageRef frame = CGImageSourceCreateImageAtIndex(source, index, (__bridge CFDictionaryRef)uncached);
            while (frame && !input.readyForMoreMediaData && writer.status == AVAssetWriterStatusWriting &&
                   dispatch_time(DISPATCH_TIME_NOW, 0) < deadline) {
                usleep(2000);
            }
            CVPixelBufferRef buffer = NULL;
            written = frame && input.readyForMoreMediaData && adaptor.pixelBufferPool &&
                CVPixelBufferPoolCreatePixelBuffer(NULL, adaptor.pixelBufferPool, &buffer) == kCVReturnSuccess;
            if (written) {
                CVPixelBufferLockBaseAddress(buffer, 0);
                CGContextRef context = CGBitmapContextCreate(
                    CVPixelBufferGetBaseAddress(buffer), width, height, 8, CVPixelBufferGetBytesPerRow(buffer),
                    space, kCGImageAlphaPremultipliedFirst | kCGBitmapByteOrder32Little);
                written = context != NULL;
                if (context) {
                    // Video has no alpha: transparent pixels show the page white they were copied from.
                    CGRect bounds = CGRectMake(0, 0, width, height);
                    CGContextSetRGBFillColor(context, 1, 1, 1, 1);
                    CGContextFillRect(context, bounds);
                    CGContextSetInterpolationQuality(context, kCGInterpolationHigh);
                    CGContextDrawImage(context, bounds, frame);
                    CGContextRelease(context);
                }
                CVPixelBufferUnlockBaseAddress(buffer, 0);
                written = written && [adaptor appendPixelBuffer:buffer withPresentationTime:CMTimeMake(elapsed, 1000)];
                elapsed += animation_delay_ms(source, index);
            }
            if (buffer) CVPixelBufferRelease(buffer);
            if (frame) CGImageRelease(frame);
        }
    }
    CGColorSpaceRelease(space);
    if (!written) {
        [writer cancelWriting];
        return false;
    }
    [input markAsFinished];
    // The last frame keeps its own delay before the clip loops.
    [writer endSessionAtSourceTime:CMTimeMake(elapsed, 1000)];
    dispatch_semaphore_t finished = dispatch_semaphore_create(0);
    [writer finishWritingWithCompletionHandler:^{
        dispatch_semaphore_signal(finished);
    }];
    if (dispatch_semaphore_wait(finished, deadline)) return false;
    return writer.status == AVAssetWriterStatusCompleted;
}

// Converts an animated GIF or WebP into the looping H.264 MP4 that WhatsApp
// sends as a GIF. Runs only on bounded upload workers; `output` must not exist.
// Returns false for still images and anything that cannot be encoded.
bool whatsapp_animation_video(const uint8_t *bytes, size_t length, const char *output) {
    @autoreleasepool {
        @try {
            if (!bytes || length == 0 || !output) return false;
            NSString *path = [[NSFileManager defaultManager] stringWithFileSystemRepresentation:output length:strlen(output)];
            if (!path) return false;
            NSData *data = [NSData dataWithBytesNoCopy:(void *)bytes length:length freeWhenDone:NO];
            CGImageSourceRef source = CGImageSourceCreateWithData((__bridge CFDataRef)data, NULL);
            if (!source) return false;
            bool encoded = animation_video(source, [NSURL fileURLWithPath:path]);
            CFRelease(source);
            return encoded;
        } @catch (NSException *exception) {
            return false; // Never allow Objective-C exceptions to cross Rust.
        }
    }
}

#if TARGET_OS_OSX
// A separate local preview never changes the transmitted/archive thumbnail.
bool whatsapp_video_preview(const char *path, double *seconds, uint32_t *width,
                            uint32_t *height, void **jpeg, size_t *length) {
    return video_poster(path, seconds, width, height, jpeg, length, 640, 512 * 1024, 0.85);
}

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
