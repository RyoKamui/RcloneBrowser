#include "osx_helper.h"
#include <Cocoa/Cocoa.h>
#include <ApplicationServices/ApplicationServices.h>
#include <QIcon>
#include <QFileIconProvider>

QIcon osxGetIcon(const QString& extension)
{
    (void)extension; return QIcon(); // Return empty to fallback to generic icon
}

void osxShowDockIcon()
{
    ProcessSerialNumber psn = { 0, kCurrentProcess };
    TransformProcessType(&psn, kProcessTransformToForegroundApplication);
}

void osxHideDockIcon()
{
    ProcessSerialNumber psn = { 0, kCurrentProcess };
    TransformProcessType(&psn, kProcessTransformToUIElementApplication);
}
