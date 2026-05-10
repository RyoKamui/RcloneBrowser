#include <QImage>
#include <CoreGraphics/CoreGraphics.h>
void test() {
    CGImageRef imageRef = nullptr;
    QImage img = QImage::fromCGImage(imageRef);
}
