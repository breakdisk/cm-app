package io.logisticos.driver.core.database.entity

import androidx.room.TypeConverter

class Converters {
    @TypeConverter fun fromTaskStatus(v: TaskStatus): String = v.name
    @TypeConverter fun toTaskStatus(v: String): TaskStatus = TaskStatus.valueOf(v)
    @TypeConverter fun fromTaskType(v: TaskType): String = v.name
    @TypeConverter fun toTaskType(v: String): TaskType = TaskType.valueOf(v)
    @TypeConverter fun fromSyncAction(v: SyncAction): String = v.name
    @TypeConverter fun toSyncAction(v: String): SyncAction = SyncAction.valueOf(v)
}
