using System;
using System.ComponentModel;
using System.Diagnostics;
using System.Runtime.InteropServices;
using System.Text;
using System.Threading;

public static class WindowsTestJob
{
    public static int Run(string executable, string arguments, string directory, int timeoutSeconds)
    {
        IntPtr job = CreateJobObject(IntPtr.Zero, null);
        Check(job != IntPtr.Zero);
        ProcessInfo process = new ProcessInfo();
        bool assigned = false;
        try
        {
            var limits = new ExtendedLimits();
            limits.Basic.Flags = 0x2000; // KILL_ON_JOB_CLOSE
            Check(SetInformationJobObject(job, 9, ref limits, Marshal.SizeOf(limits)));
            var startup = new StartupInfo();
            startup.Size = Marshal.SizeOf(startup);
            // A suspended child cannot create descendants before it joins the job.
            var command = new StringBuilder("\"" + executable + "\" " + arguments);
            Check(CreateProcess(executable, command, IntPtr.Zero, IntPtr.Zero, true,
                4, IntPtr.Zero, directory, ref startup, out process));
            Check(AssignProcessToJobObject(job, process.Process));
            assigned = true;
            Check(ResumeThread(process.Thread) != uint.MaxValue);

            uint wait = WaitForSingleObject(process.Process, checked((uint)timeoutSeconds * 1000));
            if (wait == 258) throw new TimeoutException("Test process timed out after " + timeoutSeconds + " seconds");
            Check(wait == 0);
            uint exitCode;
            Check(GetExitCodeProcess(process.Process, out exitCode));
            return unchecked((int)exitCode);
        }
        finally
        {
            try
            {
                if (assigned) StopJob(job);
                else if (process.Process != IntPtr.Zero) Check(TerminateProcess(process.Process, 1));
            }
            finally
            {
                if (process.Thread != IntPtr.Zero) CloseHandle(process.Thread);
                if (process.Process != IntPtr.Zero) CloseHandle(process.Process);
                CloseHandle(job);
            }
        }
    }

    private static void StopJob(IntPtr job)
    {
        Accounting accounting;
        int size = Marshal.SizeOf(typeof(Accounting));
        Check(QueryInformationJobObject(job, 1, out accounting, size, IntPtr.Zero));
        if (accounting.Active == 0) return;
        Check(TerminateJobObject(job, 1));
        var clock = Stopwatch.StartNew();
        do
        {
            Check(QueryInformationJobObject(job, 1, out accounting, size, IntPtr.Zero));
            if (accounting.Active == 0) return;
            Thread.Sleep(25);
        } while (clock.ElapsedMilliseconds < 5000);
        throw new InvalidOperationException("The test job still contains active processes after termination");
    }

    private static void Check(bool ok)
    {
        if (!ok) throw new Win32Exception(Marshal.GetLastWin32Error());
    }

    [StructLayout(LayoutKind.Sequential)]
    private struct Accounting {
        public long User, Kernel, PeriodUser, PeriodKernel;
        public uint Faults, Total, Active, Terminated;
    }
    [StructLayout(LayoutKind.Sequential)]
    private struct BasicLimits {
        public long ProcessTime, JobTime;
        public uint Flags;
        public UIntPtr MinWorkingSet, MaxWorkingSet;
        public uint ActiveLimit;
        public UIntPtr Affinity;
        public uint Priority, Scheduling;
    }
    [StructLayout(LayoutKind.Sequential)]
    private struct ExtendedLimits {
        public BasicLimits Basic;
        public ulong ReadOps, WriteOps, OtherOps, ReadBytes, WriteBytes, OtherBytes;
        public UIntPtr ProcessMemory, JobMemory, PeakProcessMemory, PeakJobMemory;
    }
    [StructLayout(LayoutKind.Sequential)]
    private struct StartupInfo {
        public int Size;
        public IntPtr Reserved, Desktop, Title;
        public uint X, Y, Width, Height, Columns, Rows, Fill, Flags;
        public ushort Show, ReservedSize;
        public IntPtr ReservedBytes, StdIn, StdOut, StdErr;
    }
    [StructLayout(LayoutKind.Sequential)]
    private struct ProcessInfo {
        public IntPtr Process, Thread;
        public uint ProcessId, ThreadId;
    }

    [DllImport("kernel32.dll", CharSet = CharSet.Unicode, SetLastError = true)]
    private static extern IntPtr CreateJobObject(IntPtr security, string name);
    [DllImport("kernel32.dll", SetLastError = true)]
    private static extern bool SetInformationJobObject(IntPtr job, int kind, ref ExtendedLimits info, int size);
    [DllImport("kernel32.dll", SetLastError = true)]
    private static extern bool QueryInformationJobObject(IntPtr job, int kind, out Accounting info, int size, IntPtr returned);
    [DllImport("kernel32.dll", CharSet = CharSet.Unicode, SetLastError = true)]
    private static extern bool CreateProcess(string file, StringBuilder command, IntPtr processAttrs, IntPtr threadAttrs,
        bool inherit, uint flags, IntPtr env, string directory, ref StartupInfo startup, out ProcessInfo process);
    [DllImport("kernel32.dll", SetLastError = true)]
    private static extern bool AssignProcessToJobObject(IntPtr job, IntPtr process);
    [DllImport("kernel32.dll", SetLastError = true)]
    private static extern uint ResumeThread(IntPtr thread);
    [DllImport("kernel32.dll", SetLastError = true)]
    private static extern uint WaitForSingleObject(IntPtr handle, uint milliseconds);
    [DllImport("kernel32.dll", SetLastError = true)]
    private static extern bool GetExitCodeProcess(IntPtr process, out uint code);
    [DllImport("kernel32.dll", SetLastError = true)]
    private static extern bool TerminateJobObject(IntPtr job, uint code);
    [DllImport("kernel32.dll", SetLastError = true)]
    private static extern bool TerminateProcess(IntPtr process, uint code);
    [DllImport("kernel32.dll")]
    private static extern bool CloseHandle(IntPtr handle);
}
