//! Process management for OXIDE OS
//!
//! Provides std::process-like APIs using OXIDE syscalls.

use alloc::string::{String, ToString};
use alloc::vec::Vec;

/// Representation of a running or exited child process
pub struct Child {
    /// Process ID
    pid: u32,
    /// Whether the process has been waited on
    waited: bool,
}

impl Child {
    /// Returns the OS-assigned process identifier
    pub fn id(&self) -> u32 {
        self.pid
    }

    /// Waits for the child to exit completely, returning the status
    pub fn wait(&mut self) -> crate::io::Result<ExitStatus> {
        if self.waited {
            return Err(crate::io::Error::new(
                crate::io::ErrorKind::InvalidInput,
                "child has already been waited on",
            ));
        }

        let mut status = 0i32;
        let result = libc::waitpid(self.pid as i32, &mut status, 0);

        if result < 0 {
            return Err(crate::io::Error::from_raw_os_error(result));
        }

        self.waited = true;
        Ok(ExitStatus { code: status })
    }

    /// Attempts to collect the exit status of the child if it has exited
    pub fn try_wait(&mut self) -> crate::io::Result<Option<ExitStatus>> {
        if self.waited {
            return Ok(None);
        }

        let mut status = 0i32;
        let result = libc::waitpid(self.pid as i32, &mut status, libc::WNOHANG);

        if result < 0 {
            return Err(crate::io::Error::from_raw_os_error(result));
        }

        if result == 0 {
            // Child hasn't exited yet
            Ok(None)
        } else {
            self.waited = true;
            Ok(Some(ExitStatus { code: status }))
        }
    }

    /// Forces the child process to exit
    pub fn kill(&mut self) -> crate::io::Result<()> {
        let result = libc::sys_kill(self.pid as i32, libc::signal::SIGKILL);
        if result < 0 {
            Err(crate::io::Error::from_raw_os_error(result))
        } else {
            Ok(())
        }
    }
}

impl Drop for Child {
    fn drop(&mut self) {
        // If we haven't waited on the child, it will become a zombie
        // In a full implementation, we might want to detach it
    }
}

/// Describes the result of a process after it has terminated
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ExitStatus {
    code: i32,
}

impl ExitStatus {
    /// Returns the exit code of the process
    pub fn code(&self) -> Option<i32> {
        if libc::wifexited(self.code) {
            Some(libc::wexitstatus(self.code))
        } else {
            None
        }
    }

    /// Returns true if the process exited successfully
    pub fn success(&self) -> bool {
        self.code() == Some(0)
    }

    /// Returns the signal that terminated the process, if any
    pub fn signal(&self) -> Option<i32> {
        if libc::wifsignaled(self.code) {
            Some(libc::wtermsig(self.code))
        } else {
            None
        }
    }
}

impl core::fmt::Display for ExitStatus {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        if let Some(code) = self.code() {
            write!(f, "exit code: {}", code)
        } else if let Some(signal) = self.signal() {
            write!(f, "signal: {}", signal)
        } else {
            write!(f, "exit status: {}", self.code)
        }
    }
}

/// A process builder, providing fine-grained control over spawning
pub struct Command {
    program: String,
    args: Vec<String>,
    env: Vec<(String, String)>,
    cwd: Option<String>,
}

impl Command {
    /// Constructs a new Command for launching the program at path `program`
    pub fn new(program: &str) -> Self {
        Command {
            program: program.to_string(),
            args: Vec::new(),
            env: Vec::new(),
            cwd: None,
        }
    }

    /// Adds an argument to pass to the program
    pub fn arg(&mut self, arg: &str) -> &mut Self {
        self.args.push(arg.to_string());
        self
    }

    /// Adds multiple arguments
    pub fn args<I, S>(&mut self, args: I) -> &mut Self
    where
        I: IntoIterator<Item = S>,
        S: AsRef<str>,
    {
        for arg in args {
            self.args.push(arg.as_ref().to_string());
        }
        self
    }

    /// Sets an environment variable
    pub fn env(&mut self, key: &str, val: &str) -> &mut Self {
        self.env.push((key.to_string(), val.to_string()));
        self
    }

    /// Sets the working directory for the child process
    pub fn current_dir(&mut self, dir: &str) -> &mut Self {
        self.cwd = Some(dir.to_string());
        self
    }

    /// Executes the command as a child process, returning a handle to it
    pub fn spawn(&mut self) -> crate::io::Result<Child> {
        // Fork
        let pid = libc::fork();

        if pid < 0 {
            return Err(crate::io::Error::from_raw_os_error(pid));
        }

        if pid == 0 {
            // Child process

            // Change directory if specified
            if let Some(ref dir) = self.cwd {
                libc::chdir(dir);
            }

            // Set environment variables
            for (key, val) in &self.env {
                libc::setenv(key, val);
            }

            // Execute the program
            libc::exec(&self.program);

            // If exec returns, it failed
            libc::_exit(127);
        }

        // Parent process
        Ok(Child {
            pid: pid as u32,
            waited: false,
        })
    }

    /// Executes the command as a child process, waiting for it to finish
    pub fn status(&mut self) -> crate::io::Result<ExitStatus> {
        self.spawn()?.wait()
    }

    /// Executes the command as a child process, capturing its output
    pub fn output(&mut self) -> crate::io::Result<Output> {
        // For now, just run and return empty output
        // A full implementation would set up pipes
        let status = self.status()?;
        Ok(Output {
            status,
            stdout: Vec::new(),
            stderr: Vec::new(),
        })
    }
}

/// The output of a finished process
pub struct Output {
    /// The exit status
    pub status: ExitStatus,
    /// The data that the process wrote to stdout
    pub stdout: Vec<u8>,
    /// The data that the process wrote to stderr
    pub stderr: Vec<u8>,
}

/// Terminates the current process with the specified exit code
pub fn exit(code: i32) -> ! {
    libc::_exit(code)
}

/// Terminates the current process in an abnormal fashion
pub fn abort() -> ! {
    libc::sys_kill(libc::sys_getpid(), libc::signal::SIGABRT);
    libc::_exit(134)
}

/// Returns the OS-assigned process identifier
pub fn id() -> u32 {
    libc::getpid() as u32
}

/// Returns the parent process's identifier
pub fn parent_id() -> u32 {
    libc::getppid() as u32
}
