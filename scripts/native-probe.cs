public static class WtmNative
{
    [System.Runtime.InteropServices.StructLayout(System.Runtime.InteropServices.LayoutKind.Sequential)]
    public struct Rect { public int Left, Top, Right, Bottom; }
    [System.Runtime.InteropServices.StructLayout(System.Runtime.InteropServices.LayoutKind.Sequential)]
    public struct Point { public int X, Y; }
    [System.Runtime.InteropServices.StructLayout(System.Runtime.InteropServices.LayoutKind.Sequential)]
    public struct Size { public int Width, Height; }
    [System.Runtime.InteropServices.StructLayout(System.Runtime.InteropServices.LayoutKind.Sequential, CharSet = System.Runtime.InteropServices.CharSet.Unicode)]
    public struct LogFont {
        public int Height, Width, Escapement, Orientation, Weight;
        public byte Italic, Underline, StrikeOut, CharSet, OutPrecision, ClipPrecision, Quality, PitchAndFamily;
        [System.Runtime.InteropServices.MarshalAs(System.Runtime.InteropServices.UnmanagedType.ByValTStr, SizeConst = 32)]
        public string FaceName;
    }
    [System.Runtime.InteropServices.DllImport("user32.dll")] static extern System.IntPtr SendMessageW(System.IntPtr window, uint message, System.IntPtr wparam, System.IntPtr lparam);
    [System.Runtime.InteropServices.DllImport("user32.dll")] static extern System.IntPtr GetDlgItem(System.IntPtr window, int id);
    [System.Runtime.InteropServices.DllImport("user32.dll")] static extern System.IntPtr GetDC(System.IntPtr window);
    [System.Runtime.InteropServices.DllImport("user32.dll")] static extern int ReleaseDC(System.IntPtr window, System.IntPtr dc);
    [System.Runtime.InteropServices.DllImport("user32.dll")] static extern bool GetWindowRect(System.IntPtr window, out Rect rect);
    [System.Runtime.InteropServices.DllImport("user32.dll")] static extern bool SetCursorPos(int x, int y);
    [System.Runtime.InteropServices.DllImport("user32.dll")] static extern bool IsWindowVisible(System.IntPtr window);
    [System.Runtime.InteropServices.DllImport("user32.dll")] static extern uint GetDpiForWindow(System.IntPtr window);
    [System.Runtime.InteropServices.DllImport("gdi32.dll", CharSet = System.Runtime.InteropServices.CharSet.Unicode)] static extern int GetObjectW(System.IntPtr font, int size, out LogFont value);
    [System.Runtime.InteropServices.DllImport("gdi32.dll", CharSet = System.Runtime.InteropServices.CharSet.Unicode)] static extern System.IntPtr CreateFontIndirectW(ref LogFont font);
    [System.Runtime.InteropServices.DllImport("gdi32.dll")] static extern System.IntPtr SelectObject(System.IntPtr dc, System.IntPtr value);
    [System.Runtime.InteropServices.DllImport("gdi32.dll")] static extern bool DeleteObject(System.IntPtr value);
    [System.Runtime.InteropServices.DllImport("gdi32.dll", CharSet = System.Runtime.InteropServices.CharSet.Unicode)] static extern bool GetTextExtentPoint32W(System.IntPtr dc, string text, int count, out Size size);
    public static System.IntPtr CloneFont(System.IntPtr control) {
        var handle = SendMessageW(control, 0x0031, System.IntPtr.Zero, System.IntPtr.Zero);
        LogFont font;
        if (handle == System.IntPtr.Zero || GetObjectW(handle, System.Runtime.InteropServices.Marshal.SizeOf(typeof(LogFont)), out font) == 0)
            throw new System.InvalidOperationException("Control font is unavailable");
        var clone = CreateFontIndirectW(ref font);
        if (clone == System.IntPtr.Zero) throw new System.InvalidOperationException("Control font could not be copied");
        return clone;
    }
    public static Size Measure(System.IntPtr control, string text) {
        var font = CloneFont(control);
        var dc = GetDC(control);
        if (dc == System.IntPtr.Zero) { DeleteObject(font); throw new System.InvalidOperationException("Control device context is unavailable"); }
        var previous = SelectObject(dc, font);
        try {
            Size result;
            if (!GetTextExtentPoint32W(dc, text, text.Length, out result)) throw new System.InvalidOperationException("Text measurement failed");
            return result;
        } finally { SelectObject(dc, previous); ReleaseDC(control, dc); DeleteObject(font); }
    }
    public static int Inset(System.IntPtr control, int width) {
        return System.Math.Min(System.Math.Max(0, 14 * (int)GetDpiForWindow(control) / 96 - Measure(control, " ").Width), (width - 1) / 2);
    }
    public static void Opaque(string profile) {
        var directory = System.IO.Path.Combine(profile, ".config", "wtm");
        System.IO.Directory.CreateDirectory(directory);
        System.IO.File.WriteAllText(System.IO.Path.Combine(directory, "config.opacity.json"), "100");
    }
    public static void Hover(System.IntPtr window) {
        var status = GetDlgItem(window, 1002);
        Rect bounds;
        if (status == System.IntPtr.Zero || !GetWindowRect(status, out bounds) || !SetCursorPos((bounds.Left + bounds.Right) / 2, (bounds.Top + bounds.Bottom) / 2))
            throw new System.InvalidOperationException("Window hover target is unavailable");
        var deadline = System.DateTime.UtcNow.AddSeconds(3);
        while (!IsWindowVisible(GetDlgItem(window, 1004))) {
            if (System.DateTime.UtcNow >= deadline) throw new System.InvalidOperationException("Controls did not appear on hover");
            System.Threading.Thread.Sleep(10);
        }
    }
    [System.Runtime.InteropServices.StructLayout(System.Runtime.InteropServices.LayoutKind.Sequential)]
    public struct ScrollInfo { public uint Size, Mask; public int Min, Max; public uint Page; public int Position, TrackPosition; }
    [System.Runtime.InteropServices.DllImport("user32.dll")] static extern bool GetScrollInfo(System.IntPtr window, int bar, ref ScrollInfo info);
    [System.Runtime.InteropServices.DllImport("user32.dll")] static extern bool SetWindowPos(System.IntPtr window, System.IntPtr after, int x, int y, int width, int height, uint flags);
    public static void RequireOverflow(System.IntPtr window) {
        var scrollbar = GetDlgItem(window, 1003);
        var info = new ScrollInfo { Size = (uint)System.Runtime.InteropServices.Marshal.SizeOf(typeof(ScrollInfo)), Mask = 3 };
        Rect bounds;
        if (!GetScrollInfo(scrollbar, 2, ref info) || !GetWindowRect(window, out bounds)) throw new System.InvalidOperationException("Scroll geometry is unavailable");
        int content = info.Max - info.Min + 1;
        if (content <= 1) throw new System.InvalidOperationException("Scrollable fixture has no content");
        if (content <= info.Page) {
            int height = bounds.Bottom - bounds.Top - (int)info.Page + content / 2;
            if (!SetWindowPos(window, System.IntPtr.Zero, 0, 0, bounds.Right - bounds.Left, height, 0x16)) throw new System.InvalidOperationException("Scrollable fixture resize failed");
            if (!GetScrollInfo(scrollbar, 2, ref info) || info.Max - info.Min + 1 <= info.Page) throw new System.InvalidOperationException("Fixture does not overflow");
        }
        WakeScrollbar(window);
    }
    delegate bool EnumWindow(System.IntPtr window, System.IntPtr data);
    [System.Runtime.InteropServices.DllImport("user32.dll")] static extern bool EnumWindows(EnumWindow callback, System.IntPtr data);
    [System.Runtime.InteropServices.DllImport("user32.dll")] static extern System.IntPtr GetWindow(System.IntPtr window, uint command);
    [System.Runtime.InteropServices.DllImport("user32.dll", CharSet = System.Runtime.InteropServices.CharSet.Unicode)] static extern int GetClassNameW(System.IntPtr window, System.Text.StringBuilder name, int count);
    [System.Runtime.InteropServices.DllImport("user32.dll")] static extern bool GetCursorPos(out Point point);
    [System.Runtime.InteropServices.DllImport("user32.dll")] static extern System.IntPtr WindowFromPoint(Point point);
    [System.Runtime.InteropServices.DllImport("user32.dll")] static extern System.IntPtr GetAncestor(System.IntPtr window, uint flags);
    [System.Runtime.InteropServices.DllImport("user32.dll")] static extern bool GetClientRect(System.IntPtr window, out Rect rect);
    [System.Runtime.InteropServices.DllImport("user32.dll")] static extern bool ClientToScreen(System.IntPtr window, ref Point point);
    public static void WaitForPresentation(System.IntPtr window) {
        System.IntPtr overlay = System.IntPtr.Zero;
        EnumWindows((candidate, data) => {
            if (GetWindow(candidate, 4) != window) return true;
            var name = new System.Text.StringBuilder(64);
            GetClassNameW(candidate, name, name.Capacity);
            if (name.ToString() == "WtmIndicatorOverlay") { overlay = candidate; return false; }
            return true;
        }, System.IntPtr.Zero);
        if (overlay == System.IntPtr.Zero) throw new System.InvalidOperationException("Indicator overlay is unavailable");
        var deadline = System.DateTime.UtcNow.AddSeconds(3);
        do {
            Point cursor;
            Rect client, actual;
            var origin = new Point();
            if (!GetCursorPos(out cursor) || !GetClientRect(window, out client) || !ClientToScreen(window, ref origin) || !GetWindowRect(overlay, out actual))
                throw new System.InvalidOperationException("Presentation geometry is unavailable");
            var hit = GetAncestor(WindowFromPoint(cursor), 2);
            bool hover = hit == window || hit == overlay;
            bool controls = IsWindowVisible(GetDlgItem(window, 1004));
            bool ready = controls ? !IsWindowVisible(overlay) : IsWindowVisible(overlay) && actual.Left == origin.X && actual.Top == origin.Y && actual.Right - actual.Left == client.Right && actual.Bottom - actual.Top == client.Bottom;
            if (hover == controls && ready) return;
            System.Threading.Thread.Sleep(10);
        } while (System.DateTime.UtcNow < deadline);
        throw new System.InvalidOperationException("Hover state and indicator overlay did not settle after layout");
    }
    public static void WakeScrollbar(System.IntPtr window) {
        SendMessageW(GetDlgItem(window, 1001), 0x0115, new System.IntPtr(8), System.IntPtr.Zero);
    }
}
