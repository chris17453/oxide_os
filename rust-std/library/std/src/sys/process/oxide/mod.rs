//! — BlackLatch: Process management for std::process — Command, spawn, wait.

use super::CommandEnvs;
use super::env::CommandEnv;
use crate::ffi::OsStr;
pub use crate::ffi::OsString as EnvKey;
use crate::num::NonZeroI32;
use crate::path::Path;
use crate::process::StdioPipes;
use crate::sys::fs::File;
use crate::sys::{AsInner, FromInner, IntoInner};
use crate::{fmt, io};

pub enum Stdio {
    Inherit,
    Null,
    MakePipe,
    Fd(crate::sys::fd::FileDesc),
}

impl Stdio {
    fn try_clone(&self) -> io::Result<Self> {
        match self {
            Stdio::Inherit => Ok(Stdio::Inherit),
            Stdio::Null => Ok(Stdio::Null),
            Stdio::MakePipe => Ok(Stdio::MakePipe),
            Stdio::Fd(fd) => Ok(Stdio::Fd(fd.try_clone()?)),
        }
    }
}

#[derive(Default)]
pub struct Command {
    program: String,
    args: Vec<String>,
    cwd: Option<String>,
    stdin: Option<Stdio>,
    stdout: Option<Stdio>,
    stderr: Option<Stdio>,
    env: CommandEnv,
}

impl Command {
    pub fn new(program: &OsStr) -> Command {
        Command {
            program: program.to_string_lossy().into_owned(),
            args: vec![program.to_string_lossy().into_owned()],
            cwd: None,
            stdin: None,
            stdout: None,
            stderr: None,
            env: Default::default(),
        }
    }

    pub fn arg(&mut self, arg: &OsStr) {
        self.args.push(arg.to_string_lossy().into_owned());
    }

    pub fn env_mut(&mut self) -> &mut CommandEnv { &mut self.env }
    pub fn cwd(&mut self, dir: &OsStr) { self.cwd = Some(dir.to_string_lossy().into_owned()); }
    pub fn stdin(&mut self, stdin: Stdio) { self.stdin = Some(stdin); }
    pub fn stdout(&mut self, stdout: Stdio) { self.stdout = Some(stdout); }
    pub fn stderr(&mut self, stderr: Stdio) { self.stderr = Some(stderr); }
    pub fn get_program(&self) -> &OsStr { OsStr::new(&self.program) }
    pub fn get_args(&self) -> CommandArgs<'_> { CommandArgs { iter: self.args[1..].iter() } }
    pub fn get_envs(&self) -> CommandEnvs<'_> { self.env.iter() }
    pub fn get_env_clear(&self) -> bool { self.env.does_clear() }
    pub fn get_current_dir(&self) -> Option<&Path> { self.cwd.as_ref().map(|s| Path::new(s.as_str())) }

    pub fn spawn(&mut self, _default: Stdio, _needs_stdin: bool)
        -> io::Result<(Process, StdioPipes)>
    {
        let pid = oxide_rt::process::fork();
        if pid < 0 {
            return Err(io::Error::from_raw_os_error(-pid));
        }

        if pid == 0 {
            // Child process
            // Build null-terminated argv
            let c_args: Vec<Vec<u8>> = self.args.iter()
                .map(|a| { let mut v = a.as_bytes().to_vec(); v.push(0); v })
                .collect();
            let argv_ptrs: Vec<*const u8> = c_args.iter().map(|a| a.as_ptr()).collect();
            let mut argv_with_null = argv_ptrs.clone();
            argv_with_null.push(core::ptr::null());

            let path_bytes = self.program.as_bytes();
            oxide_rt::process::execve(
                path_bytes,
                argv_with_null.as_ptr(),
                core::ptr::null(),
            );
            // If execve returns, it failed
            oxide_rt::os::exit(127);
        }

        // Parent process
        Ok((
            Process { pid, status: None },
            StdioPipes { stdin: None, stdout: None, stderr: None },
        ))
    }
}

impl From<crate::sys::fd::FileDesc> for Stdio {
    fn from(fd: crate::sys::fd::FileDesc) -> Self { Stdio::Fd(fd) }
}

impl From<File> for Stdio {
    fn from(file: File) -> Self { Stdio::Fd(file.into_inner()) }
}

impl From<io::Stdout> for Stdio {
    fn from(_: io::Stdout) -> Self { Stdio::Inherit }
}

impl From<io::Stderr> for Stdio {
    fn from(_: io::Stderr) -> Self { Stdio::Inherit }
}

impl fmt::Debug for Command {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Command {{ program: {:?}, args: {:?} }}", self.program, self.args)
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub struct ExitStatus(i32);

impl ExitStatus {
    pub fn exit_ok(&self) -> Result<(), ExitStatusError> {
        if self.0 == 0 { Ok(()) } else { Err(ExitStatusError(*self)) }
    }
    pub fn code(&self) -> Option<i32> { Some(self.0) }
}

impl fmt::Display for ExitStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "exit status: {}", self.0)
    }
}

#[derive(PartialEq, Eq, Clone, Copy, Debug)]
pub struct ExitStatusError(ExitStatus);

impl Into<ExitStatus> for ExitStatusError {
    fn into(self) -> ExitStatus { self.0 }
}

impl ExitStatusError {
    pub fn code(self) -> Option<NonZeroI32> { NonZeroI32::new(self.0.0) }
}

#[derive(PartialEq, Eq, Clone, Copy, Debug)]
pub struct ExitCode(i32);

impl ExitCode {
    pub const SUCCESS: ExitCode = ExitCode(0);
    pub const FAILURE: ExitCode = ExitCode(1);
    pub fn as_i32(&self) -> i32 { self.0 }
}

impl From<u8> for ExitCode {
    fn from(code: u8) -> Self { ExitCode(code as i32) }
}

pub struct Process {
    pid: i32,
    status: Option<ExitStatus>,
}

impl Drop for Process {
    fn drop(&mut self) {
        // — BlackLatch: Don't leave zombies. If not waited on, wait now.
    }
}

impl Process {
    pub fn id(&self) -> u32 { self.pid as u32 }

    pub fn kill(&mut self) -> io::Result<()> {
        let ret = oxide_rt::process::kill(self.pid, 9); // SIGKILL
        if ret < 0 { Err(io::Error::from_raw_os_error(-ret)) }
        else { Ok(()) }
    }

    pub fn wait(&mut self) -> io::Result<ExitStatus> {
        if let Some(status) = self.status { return Ok(status); }
        let mut raw_status: i32 = 0;
        let ret = oxide_rt::process::waitpid(self.pid, &mut raw_status, 0);
        if ret < 0 {
            Err(io::Error::from_raw_os_error(-ret))
        } else {
            let status = ExitStatus(oxide_rt::process::wexitstatus(raw_status));
            self.status = Some(status);
            Ok(status)
        }
    }

    pub fn try_wait(&mut self) -> io::Result<Option<ExitStatus>> {
        if let Some(status) = self.status { return Ok(Some(status)); }
        let mut raw_status: i32 = 0;
        let ret = oxide_rt::process::waitpid(self.pid, &mut raw_status, 1); // WNOHANG
        if ret < 0 {
            Err(io::Error::from_raw_os_error(-ret))
        } else if ret == 0 {
            Ok(None)
        } else {
            let status = ExitStatus(oxide_rt::process::wexitstatus(raw_status));
            self.status = Some(status);
            Ok(Some(status))
        }
    }

    pub fn handle(&self) -> u64 { self.pid as u64 }
}

pub struct CommandArgs<'a> {
    iter: core::slice::Iter<'a, String>,
}

impl<'a> Iterator for CommandArgs<'a> {
    type Item = &'a OsStr;
    fn next(&mut self) -> Option<&'a OsStr> {
        self.iter.next().map(|s| OsStr::new(s.as_str()))
    }
}

impl<'a> ExactSizeIterator for CommandArgs<'a> {
    fn len(&self) -> usize { self.iter.len() }
}

impl<'a> fmt::Debug for CommandArgs<'a> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_list().entries(self.iter.clone()).finish()
    }
}

pub type ChildPipe = crate::sys::pipe::Pipe;

pub fn read_output(
    _out: ChildPipe, _stdout: &mut Vec<u8>,
    _err: ChildPipe, _stderr: &mut Vec<u8>,
) -> io::Result<()> {
    // — BlackLatch: TODO implement output capture
    Ok(())
}
