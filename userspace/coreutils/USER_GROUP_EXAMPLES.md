# User and Group Management Commands - Usage Examples

## useradd - Create New Users

### Basic Usage
```bash
# Create a user with auto-assigned UID/GID
useradd alice
```

### With Home Directory
```bash
# Create user and their home directory
useradd -m bob
```

### Full Options
```bash
# Create user with all options specified
useradd -u 2000 -g 2000 -d /home/charlie -s /bin/esh -c "Charlie Brown" -m charlie
```

### Options
- `-u UID` - Specify user ID (default: auto-assigned starting from 1000)
- `-g GID` - Specify primary group ID (default: same as UID)
- `-d HOME` - Specify home directory (default: /home/username)
- `-s SHELL` - Specify login shell (default: /bin/esh)
- `-c COMMENT` - Specify GECOS comment field (user's full name, etc.)
- `-m` - Create home directory
- `-h, --help` - Show help message

## groupadd - Create New Groups

### Basic Usage
```bash
# Create a group with auto-assigned GID
groupadd developers
```

### With Specific GID
```bash
# Create group with specific GID
groupadd -g 3000 admins
```

### System Group
```bash
# Create system group (GID < 1000)
groupadd -r service
```

### Options
- `-g GID` - Specify group ID (default: auto-assigned)
  - Regular groups start from 1000
  - System groups (with -r) start from 100
- `-r` - Create system group
- `-h, --help` - Show help message

## Example Workflow

### Setting up a new user environment
```bash
# 1. Create a group for the project
groupadd -g 5000 webdev

# 2. Create a user in that group
useradd -u 5001 -g 5000 -m -c "Web Developer" -s /bin/esh webuser

# 3. Verify the setup
cat /etc/passwd | grep webuser
cat /etc/group | grep webdev
ls -ld /home/webuser
```

Output:
```
webuser:x:5001:5000:Web Developer:/home/webuser:/bin/esh
webdev:x:5000:
drwxr-xr-x 2 webuser webdev 4096 ... /home/webuser
```

### Creating multiple users
```bash
# Create users for a team
useradd -m alice
useradd -m bob
useradd -m charlie

# List all users
cat /etc/passwd
```

## Integration with Other Commands

### Check User Info
```bash
# After creating a user
useradd -m testuser

# View user ID
id testuser

# Switch to user (if switching is supported)
# su testuser
```

### File Ownership
```bash
# Create a user
useradd -m alice

# Create files as root
touch /home/alice/file.txt

# Change ownership to alice
chown 1000:1000 /home/alice/file.txt
```

## Error Handling

### Duplicate User
```bash
useradd alice
useradd alice  # Error: user 'alice' already exists
```

### Invalid UID
```bash
useradd -u abc alice  # Error: invalid UID
```

### Missing Argument
```bash
useradd -u  # Error: option requires an argument -- 'u'
```

### Permission Denied
```bash
# As non-root user
useradd alice  # Error: permission denied (must be root)
```

## File Format Reference

After running `useradd`, entries are appended to `/etc/passwd`:
```
username:x:uid:gid:gecos:home:shell
```

After running `groupadd`, entries are appended to `/etc/group`:
```
groupname:x:gid:members
```

## Notes

1. **Permissions**: Both commands require root privileges (UID 0)
2. **Auto-increment**: UIDs and GIDs are automatically incremented from existing entries
3. **Home creation**: Use `-m` flag to automatically create home directories
4. **Ownership**: Home directories are automatically chowned to the new user
5. **Duplicate prevention**: Commands check for existing users/groups before adding
6. **File safety**: Entries are appended, never overwriting existing data
