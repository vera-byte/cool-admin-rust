//! EPS 元数据启动注册（对齐 cool-admin-go 路由扫描结果）

/// 注册全部内置服务的 EPS 元数据
pub fn register_all() {
    use cool_core::eps::register_module;

    register_module(
        "base",
        "base.v1.BaseOpen",
        &[
            ("Captcha", "base.v1.BaseOpenService/Captcha", "获取验证码"),
            ("Login", "base.v1.BaseOpenService/Login", "登录"),
            (
                "RefreshToken",
                "base.v1.BaseOpenService/RefreshToken",
                "刷新令牌",
            ),
            ("Logout", "base.v1.BaseOpenService/Logout", "退出登录"),
            (
                "PersonInfo",
                "base.v1.BaseOpenService/PersonInfo",
                "个人信息",
            ),
            (
                "PersonUpdate",
                "base.v1.BaseOpenService/PersonUpdate",
                "更新个人信息",
            ),
            (
                "PersonPassword",
                "base.v1.BaseOpenService/PersonPassword",
                "修改密码",
            ),
            ("Eps", "base.v1.BaseOpenService/Eps", "接口权限表"),
            ("EpsApp", "base.v1.BaseOpenService/EpsApp", "App 接口权限表"),
        ],
    );

    register_module(
        "base",
        "base.v1.BaseSysUser",
        &[
            ("Page", "base.v1.BaseSysUserService/Page", "用户分页"),
            ("List", "base.v1.BaseSysUserService/List", "用户列表"),
            ("Info", "base.v1.BaseSysUserService/Info", "用户详情"),
            ("Add", "base.v1.BaseSysUserService/Add", "新增用户"),
            ("Update", "base.v1.BaseSysUserService/Update", "更新用户"),
            ("Delete", "base.v1.BaseSysUserService/Delete", "删除用户"),
        ],
    );

    register_module(
        "base",
        "base.v1.BaseSysRole",
        &[
            ("Page", "base.v1.BaseSysRoleService/Page", "角色分页"),
            ("List", "base.v1.BaseSysRoleService/List", "角色列表"),
            ("Info", "base.v1.BaseSysRoleService/Info", "角色详情"),
            ("Add", "base.v1.BaseSysRoleService/Add", "新增角色"),
            ("Update", "base.v1.BaseSysRoleService/Update", "更新角色"),
            ("Delete", "base.v1.BaseSysRoleService/Delete", "删除角色"),
        ],
    );

    register_module(
        "base",
        "base.v1.BaseSysDepartment",
        &[
            ("List", "base.v1.BaseSysDepartmentService/List", "部门列表"),
            ("Tree", "base.v1.BaseSysDepartmentService/Tree", "部门树"),
            ("Info", "base.v1.BaseSysDepartmentService/Info", "部门详情"),
            ("Add", "base.v1.BaseSysDepartmentService/Add", "新增部门"),
            (
                "Update",
                "base.v1.BaseSysDepartmentService/Update",
                "更新部门",
            ),
            (
                "Delete",
                "base.v1.BaseSysDepartmentService/Delete",
                "删除部门",
            ),
        ],
    );

    register_module(
        "base",
        "base.v1.BaseSysMenu",
        &[
            ("List", "base.v1.BaseSysMenuService/List", "菜单列表"),
            ("Tree", "base.v1.BaseSysMenuService/Tree", "菜单树"),
            ("Info", "base.v1.BaseSysMenuService/Info", "菜单详情"),
            ("Add", "base.v1.BaseSysMenuService/Add", "新增菜单"),
            ("Update", "base.v1.BaseSysMenuService/Update", "更新菜单"),
            ("Delete", "base.v1.BaseSysMenuService/Delete", "删除菜单"),
            ("Perms", "base.v1.BaseSysMenuService/Perms", "当前用户权限"),
        ],
    );

    register_module(
        "base",
        "base.v1.BaseSysLog",
        &[("Page", "base.v1.BaseSysLogService/Page", "操作日志")],
    );

    register_module(
        "base",
        "base.v1.BaseComm",
        &[
            (
                "UploadMode",
                "base.v1.BaseCommService/UploadMode",
                "上传模式",
            ),
            ("Upload", "base.v1.BaseCommService/Upload", "上传文件"),
            ("Download", "base.v1.BaseCommService/Download", "下载文件"),
        ],
    );

    register_module(
        "dict",
        "dict.v1.DictType",
        &[
            ("Add", "dict.v1.DictTypeService/Add", "新增字典类型"),
            ("Delete", "dict.v1.DictTypeService/Delete", "删除字典类型"),
            ("Update", "dict.v1.DictTypeService/Update", "更新字典类型"),
            ("Info", "dict.v1.DictTypeService/Info", "字典类型详情"),
            ("List", "dict.v1.DictTypeService/List", "字典类型列表"),
            ("Page", "dict.v1.DictTypeService/Page", "字典类型分页"),
        ],
    );

    register_module(
        "dict",
        "dict.v1.DictInfo",
        &[
            ("Add", "dict.v1.DictInfoService/Add", "新增字典条目"),
            ("Delete", "dict.v1.DictInfoService/Delete", "删除字典条目"),
            ("Update", "dict.v1.DictInfoService/Update", "更新字典条目"),
            ("Info", "dict.v1.DictInfoService/Info", "字典条目详情"),
            ("List", "dict.v1.DictInfoService/List", "字典条目列表"),
            ("Page", "dict.v1.DictInfoService/Page", "字典条目分页"),
            ("Data", "dict.v1.DictInfoService/Data", "字典数据聚合"),
        ],
    );

    register_module(
        "space",
        "space.v1.SpaceType",
        &[
            ("Add", "space.v1.SpaceTypeService/Add", "新增分类"),
            ("Delete", "space.v1.SpaceTypeService/Delete", "删除分类"),
            ("Update", "space.v1.SpaceTypeService/Update", "更新分类"),
            ("Info", "space.v1.SpaceTypeService/Info", "分类详情"),
            ("List", "space.v1.SpaceTypeService/List", "分类列表"),
            ("Page", "space.v1.SpaceTypeService/Page", "分类分页"),
        ],
    );

    register_module(
        "space",
        "space.v1.SpaceInfo",
        &[
            ("Add", "space.v1.SpaceInfoService/Add", "新增文件索引"),
            ("Delete", "space.v1.SpaceInfoService/Delete", "删除文件索引"),
            ("Update", "space.v1.SpaceInfoService/Update", "更新文件索引"),
            ("Info", "space.v1.SpaceInfoService/Info", "文件详情"),
            ("List", "space.v1.SpaceInfoService/List", "文件列表"),
            ("Page", "space.v1.SpaceInfoService/Page", "文件分页"),
        ],
    );

    register_module(
        "task",
        "task.v1.TaskInfo",
        &[
            ("Add", "task.v1.TaskInfoService/Add", "新增任务"),
            ("Delete", "task.v1.TaskInfoService/Delete", "删除任务"),
            ("Update", "task.v1.TaskInfoService/Update", "更新任务"),
            ("Info", "task.v1.TaskInfoService/Info", "任务详情"),
            ("List", "task.v1.TaskInfoService/List", "任务列表"),
            ("Page", "task.v1.TaskInfoService/Page", "任务分页"),
            ("Start", "task.v1.TaskInfoService/Start", "启用任务"),
            ("Stop", "task.v1.TaskInfoService/Stop", "停用任务"),
            ("Once", "task.v1.TaskInfoService/Once", "立即执行"),
            ("Log", "task.v1.TaskInfoService/Log", "任务日志"),
        ],
    );

    tracing::info!("eps metadata registered");
}
